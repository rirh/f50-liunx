//! The engine's loopback API (`127.0.0.1:17891`, onecore's `pc/core/control.go`): a small HTTP/1.1 client.
//!
//! One request per connection (`Connection: close`) with a Bearer token and a JSON body with Content-Length; the
//! engine always answers with Content-Length, which bounds the read. The envelope is `{"ok":true,"data":…}`, or
//! HTTP 422 `{"ok":false,"error":"…"}` whose message (the engine's, often Chinese) becomes a `failed` error.
//!
//! The token is `<state>/proxy/token` (0600, 32 hex characters from `/dev/urandom`), made here when it is missing;
//! the engine's procd service reads the same file.

use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use f50_core::{Code, Error, Result, fsx};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The longest reply read (connections and logs are the large ones).
const REPLY_MAX: usize = 8 * 1024 * 1024;
const HEAD_MAX: usize = 16 * 1024;
const TOKEN_LEN: usize = 32;

/// The engine's API. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Engine {
    addr: SocketAddr,
    token: String,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").field("addr", &self.addr).finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(addr: SocketAddr, token: String) -> Engine {
        Engine { addr, token }
    }

    pub async fn get(&self, path: &str, timeout: Duration) -> Result<Value> {
        self.call("GET", path, None, timeout).await
    }

    pub async fn post(&self, path: &str, body: &Value, timeout: Duration) -> Result<Value> {
        self.call("POST", path, Some(body), timeout).await
    }

    /// One call, its `data`. `timeout` bounds the whole exchange.
    async fn call(&self, method: &str, path: &str, body: Option<&Value>, timeout: Duration) -> Result<Value> {
        let exchange = async {
            let mut s = TcpStream::connect(self.addr).await.map_err(|e| unreachable(&e))?;
            s.write_all(&request(method, path, &self.addr.to_string(), &self.token, body))
                .await
                .map_err(|e| unreachable(&e))?;
            let mut raw = Vec::new();
            let mut buf = vec![0u8; 16 * 1024];
            // on the heap: every caller awaits this future
            loop {
                let n = s.read(&mut buf).await.map_err(|e| Error::failed(format!("the proxy engine: {e}")))?;
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > REPLY_MAX + HEAD_MAX {
                    return Err(Error::failed("the proxy engine's reply is too large"));
                }
                if complete(&raw) {
                    break;
                }
            }
            parse_reply(&raw)
        };
        tokio::time::timeout(timeout, exchange).await.map_err(|_| Error::timeout("the proxy engine"))?
    }
}

fn unreachable(e: &std::io::Error) -> Error {
    Error::failed(format!("the proxy engine is not answering ({e})"))
}

/// Whether the connection could not even be made (the engine is not running).
pub(crate) fn is_unreachable(e: &Error) -> bool {
    e.code() == Code::Failed && e.message().starts_with("the proxy engine is not answering")
}

pub(crate) fn request(method: &str, path: &str, host: &str, token: &str, body: Option<&Value>) -> Vec<u8> {
    let payload = body.map(Value::to_string).unwrap_or_default();
    let mut r = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nAccept: application/json\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    )
    .into_bytes();
    r.extend_from_slice(payload.as_bytes());
    r
}

fn split_head(raw: &[u8]) -> Option<(&str, &[u8])> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    Some((head, &raw[end + 4..]))
}

fn content_length(head: &str) -> Option<usize> {
    head.lines().skip(1).find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case("content-length").then(|| v.trim().parse().ok())?
    })
}

/// The whole reply is in: the head and Content-Length bytes of body.
fn complete(raw: &[u8]) -> bool {
    split_head(raw).is_some_and(|(head, body)| content_length(head).is_some_and(|n| body.len() >= n))
}

/// The reply's `data`, or the engine's error.
pub(crate) fn parse_reply(raw: &[u8]) -> Result<Value> {
    let (head, body) = split_head(raw).ok_or_else(|| Error::failed("the proxy engine's reply is incomplete"))?;
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| Error::failed("the proxy engine's reply has no status"))?;
    let body = match content_length(head) {
        Some(n) if body.len() >= n => &body[..n],
        Some(_) => return Err(Error::failed("the proxy engine's reply is incomplete")),
        None => body,
    };
    if status == 401 {
        return Err(Error::failed("the proxy engine refused the token"));
    }
    let v: Value = serde_json::from_slice(body)
        .map_err(|_| Error::failed(format!("the proxy engine answered HTTP {status} without JSON")))?;
    if v["ok"] == Value::Bool(true) {
        return Ok(v.get("data").cloned().unwrap_or(Value::Null));
    }
    match v["error"].as_str() {
        Some(e) if !e.trim().is_empty() => Err(Error::failed(e)),
        _ => Err(Error::failed(format!("the proxy engine answered HTTP {status}"))),
    }
}

fn token_ok(s: &str) -> bool {
    s.len() == TOKEN_LEN && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The engine's token in `path`; made (0600) when missing or not one. `true` when it was made now.
pub(crate) fn ensure_token(path: &Path) -> Result<(String, bool)> {
    if let Some(t) = fsx::read_opt(path)?.map(|s| s.trim().to_owned()).filter(|t| token_ok(t)) {
        return Ok((t, false));
    }
    let mut bytes = [0u8; TOKEN_LEN / 2];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| Error::io("read /dev/urandom", &e))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    fsx::write_atomic(path, format!("{token}\n").as_bytes(), 0o600)?;
    Ok((token, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reply(status: &str, body: &str) -> Vec<u8> {
        format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len())
            .into_bytes()
    }

    #[test]
    fn replies() {
        assert_eq!(parse_reply(&reply("200 OK", r#"{"ok":true,"data":{"ms":42}}"#)), Ok(json!({"ms": 42})));
        let e = parse_reply(&reply("422 Unprocessable Entity", r#"{"ok":false,"error":"没有找到节点链接"}"#))
            .expect_err("error");
        assert_eq!(e.code(), Code::Failed);
        assert_eq!(e.message(), "没有找到节点链接");
        let e = parse_reply(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 13\r\n\r\nunauthorized\n").expect_err("401");
        assert!(e.message().contains("token"));
        assert!(parse_reply(b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{\"ok\":true}").is_err(), "short body");
        assert!(parse_reply(b"HTTP/1.1 200 OK\r\nContent-Le").is_err());
        assert!(parse_reply(&reply("500 Internal Server Error", "<html>")).is_err());
        // a body read past Content-Length (no further bytes are expected, but none are taken)
        let mut r = reply("200 OK", r#"{"ok":true,"data":[]}"#);
        r.extend_from_slice(b"junk");
        assert_eq!(parse_reply(&r), Ok(json!([])));
        assert!(complete(&reply("200 OK", "{}")));
        assert!(!complete(b"HTTP/1.1 200 OK\r\ncontent-length: 3\r\n\r\n{}"));
    }

    #[test]
    fn requests_carry_the_token_and_length() {
        let r = String::from_utf8(request("POST", "/v1/start", "127.0.0.1:17891", "abc", Some(&json!({"a": "é"}))))
            .expect("utf8");
        assert!(r.starts_with("POST /v1/start HTTP/1.1\r\n"));
        assert!(r.contains("Authorization: Bearer abc\r\n"));
        assert!(r.contains("Content-Length: 10\r\n"), "{r}");
        assert!(r.ends_with("\r\n\r\n{\"a\":\"é\"}"));
        let g = String::from_utf8(request("GET", "/v1/status", "h", "t", None)).expect("utf8");
        assert!(g.contains("Content-Length: 0\r\n"));
    }

    #[test]
    fn token_is_made_once() {
        let dir = std::env::temp_dir().join(format!("f50-proxy-token-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("token");
        let (t, made) = ensure_token(&path).expect("token");
        assert!(made && token_ok(&t));
        let (again, made) = ensure_token(&path).expect("token");
        assert!(!made);
        assert_eq!(t, again);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777, 0o600);
        std::fs::write(&path, "short").expect("write");
        assert!(ensure_token(&path).expect("token").1, "a bad token is replaced");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn talks_to_a_loopback_server() {
        let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else { return };
        let addr = listener.local_addr().expect("addr");
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 4096];
            let n = s.read(&mut buf).await.expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let body = r#"{"ok":true,"data":{"running":false}}"#;
            let r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            s.write_all(r.as_bytes()).await.expect("write");
            req
        });
        let e = Engine::new(addr, "tok".into());
        let v = e.get("/v1/status", Duration::from_secs(5)).await.expect("call");
        assert_eq!(v, json!({"running": false}));
        let req = server.await.expect("server");
        assert!(req.contains("Bearer tok"));
    }
}

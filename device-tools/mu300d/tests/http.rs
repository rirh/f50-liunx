//! `mu300d serve`: content type, sessions, login, the envelope over HTTP.

mod common;

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Stdio};
use std::time::Duration;

use common::{Dev, wait_until};
use serde_json::Value;

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// One request; (status, headers, JSON body).
fn post(port: u16, path: &str, headers: &[&str], body: &str) -> (u16, String, Value) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let mut req = format!("POST {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
    for h in headers {
        req.push_str(h);
        req.push_str("\r\n");
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head.to_owned(), serde_json::from_str(body).unwrap_or(Value::Null))
}

const JSON: &str = "Content-Type: application/json";

#[test]
fn sessions_and_authorization() {
    let d = Dev::new();
    fs::write(d.p("sys/etc/shadow"), "root:*:1::::::\nubuntu:$6$s$h:19000:0:99999:7:::\n").unwrap();
    d.bin(
        "mu300-pwcheck",
        "read pw; read h; if [ \"$pw\" = secret ] && [ \"$h\" = '$6$s$h' ]; then echo MATCH; exit 0; fi; echo NOMATCH; exit 1",
    );
    d.bin("mu300-cpu", "echo '{\"governor\":\"schedutil\"}'");
    let port = free_port();
    let child = d
        .command()
        .args(["serve", "--listen", &format!("127.0.0.1:{port}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _server = Server(child);
    assert!(wait_until(10, || TcpStream::connect(("127.0.0.1", port)).is_ok()), "server did not start");

    // public
    let (st, _, v) = post(port, "/api/hello", &[JSON], "");
    assert_eq!((st, v["ok"].as_bool()), (200, Some(true)));
    // no JSON content type: refused before anything runs
    let (st, _, v) = post(port, "/api/hello", &["Content-Type: text/plain"], "{}");
    assert_eq!((st, v["code"].as_str()), (400, Some("bad-request")));
    // reads and writes need a session
    let (st, _, v) = post(port, "/api/cpu.get", &[JSON], "{}");
    assert_eq!((st, v["code"].as_str()), (401, Some("unauthorized")));
    let (st, _, v) = post(port, "/api/cpu.profile", &[JSON], "{\"name\":\"balanced\"}");
    assert_eq!((st, v["code"].as_str()), (401, Some("unauthorized")));
    assert!(!d.calls().contains("mu300-cpu profile"));
    // unknown method / path
    let (st, _, v) = post(port, "/api/nope", &[JSON], "{}");
    assert_eq!((st, v["code"].as_str()), (404, Some("unknown-method")));
    let (st, _, _) = post(port, "/other", &[JSON], "{}");
    assert_eq!(st, 404);

    // a wrong password, then the right one
    let (st, _, v) = post(port, "/api/auth.login", &[JSON], "{\"password\":\"guess\"}");
    assert_eq!((st, v["code"].as_str()), (401, Some("unauthorized")));
    let (st, head, v) = post(port, "/api/auth.login", &[JSON], "{\"password\":\"secret\"}");
    assert_eq!(st, 200, "{v}");
    let token = v["data"]["token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 64);
    let lower = head.to_ascii_lowercase();
    assert!(
        lower.contains("set-cookie: mu300d_session=")
            && lower.contains("httponly")
            && lower.contains("samesite=strict")
    );
    assert!(!lower.contains("access-control-allow-origin"));

    let bearer = format!("Authorization: Bearer {token}");
    let (st, _, v) = post(port, "/api/cpu.get", &[JSON, &bearer], "{}");
    assert_eq!((st, v["data"]["cpu"]["governor"].as_str()), (200, Some("schedutil")));
    let cookie = format!("Cookie: mu300d_session={token}");
    let (st, _, v) = post(port, "/api/cpu.profile", &[JSON, &cookie], "{\"name\":\"balanced\"}");
    assert_eq!((st, v["ok"].as_bool()), (200, Some(true)), "{v}");
    let (st, _, v) = post(port, "/api/cpu.profile", &[JSON, &cookie], "{\"name\":\"x\",\"y\":1}");
    assert_eq!((st, v["code"].as_str(), v["field"].as_str()), (400, Some("unknown-field"), Some("y")));

    // logout ends it
    let (st, _, _) = post(port, "/api/auth.logout", &[JSON, &bearer], "{}");
    assert_eq!(st, 200);
    let (st, _, _) = post(port, "/api/cpu.get", &[JSON, &bearer], "{}");
    assert_eq!(st, 401);

    // throttled after repeated failures
    let mut last = 0;
    for _ in 0..6 {
        last = post(port, "/api/auth.login", &[JSON], "{\"password\":\"nope\"}").0;
    }
    assert_eq!(last, 403);
}

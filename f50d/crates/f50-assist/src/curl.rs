//! HTTPS requests by curl, as the Telegram push makes them: the URL, headers and body go on curl's stdin as a config
//! (`-K -`), never into its arguments (any user can read those in /proc), and no inherited `*_proxy` applies.

use std::time::Duration;

use f50_core::exec::Cmd;
use f50_core::{Error, Result};
use serde_json::Value;

const MAX_SECS: u64 = 20;

/// One request; `(HTTP status, body)`. A network failure is an error with curl's message.
pub(crate) async fn request(method: &str, url: &str, headers: &[String], body: Option<&str>) -> Result<(u16, String)> {
    let mut conf = format!(
        "request = {}\nurl = {}\nwrite-out = \"\\n%{{http_code}}\"\nnoproxy = \"*\"\n",
        quote(method),
        quote(url)
    );
    for h in headers {
        conf.push_str(&format!("header = {}\n", quote(h)));
    }
    if let Some(b) = body {
        conf.push_str(&format!("data-binary = {}\n", quote(b)));
    }
    let out = Cmd::new("curl")
        .args(["-sS", "--proto", "=https,http", "--max-time", &MAX_SECS.to_string(), "-K", "-"])
        .stdin(conf)
        .timeout(Duration::from_secs(MAX_SECS + 5))
        .max_output(256 * 1024)
        .run()
        .await
        .map_err(|e| Error::failed(format!("curl could not be started: {e}")))?;
    if out.timed_out {
        return Err(Error::timeout("the request"));
    }
    let (resp, code) = out.stdout.rsplit_once('\n').unwrap_or(("", out.stdout.as_str()));
    let code: u16 = code.trim().parse().unwrap_or(0);
    if out.status != Some(0) || code == 0 {
        let m = out.message();
        return Err(Error::failed(if m.is_empty() { "the server cannot be reached".into() } else { m }));
    }
    Ok((code, resp.to_owned()))
}

/// A JSON request; the reply parsed (`Null` when it is not JSON).
pub(crate) async fn json(method: &str, url: &str, headers: &[String], body: Option<&Value>) -> Result<(u16, Value)> {
    let mut h = headers.to_vec();
    h.push("Content-Type: application/json".into());
    let body = body.map(Value::to_string);
    let (code, text) = request(method, url, &h, body.as_deref()).await?;
    Ok((code, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

/// A double-quoted curl config value: backslash and quote escaped, no line break can end it early.
fn quote(s: &str) -> String {
    let mut q = String::with_capacity(s.len() + 2);
    q.push('"');
    for c in s.chars() {
        match c {
            '\\' => q.push_str("\\\\"),
            '"' => q.push_str("\\\""),
            '\n' => q.push_str("\\n"),
            '\r' => q.push_str("\\r"),
            '\t' => q.push_str("\\t"),
            c => q.push(c),
        }
    }
    q.push('"');
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_keeps_values_on_one_line() {
        assert_eq!(quote("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
    }
}

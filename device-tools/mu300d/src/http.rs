//! `mu300d serve`: the API over HTTP. `POST /api/<method>` with a JSON body, the envelope back.
//!
//! * Requests must say `Content-Type: application/json`: a cross-site form cannot, and a script from another origin
//!   would need a CORS preflight, which is never granted (no CORS headers at all).
//! * `auth.login {password}` gives a session: an HttpOnly, SameSite=Strict cookie and the same token in the reply for
//!   a `Authorization: Bearer` client. Write methods need it; read methods too unless `open_reads`.
//! * Bodies are limited to 64 KiB; at most `CONCURRENCY` methods run at once (a busy device answers `busy`).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use axum::routing::post;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::api::error::{ApiError, ApiResult, Code};
use crate::api::registry::{Grant, dispatch};
use crate::api::validate::{Field, Rule, validate};
use crate::api::{REQUEST_MAX, envelope, parse_request};
use crate::auth::Auth;
use crate::sys::Ctx;

const CONCURRENCY: usize = 8;
const COOKIE: &str = "mu300d_session";
const LOGIN: &[Field] = &[Field::secret("password", Rule::Text { max_bytes: 128, max_chars: 128 }, true)];

struct App {
    ctx: Ctx,
    auth: Auth,
    slots: Semaphore,
}

fn session_token(h: &HeaderMap) -> Option<String> {
    if let Some(b) = h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(b.trim().to_owned());
    }
    h.get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| c.trim().strip_prefix(COOKIE).and_then(|r| r.strip_prefix('=')))
        .map(str::to_owned)
}

fn is_json(h: &HeaderMap) -> bool {
    h.get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next().is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json")))
}

fn respond(r: &ApiResult<Value>, cookie: Option<String>) -> Response {
    let status = match r {
        Ok(_) => StatusCode::OK,
        Err(e) => StatusCode::from_u16(e.code.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let mut resp = Response::new(Body::from(envelope(r).to_string()));
    *resp.status_mut() = status;
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    if let Some(c) = cookie.and_then(|c| HeaderValue::from_str(&c).ok()) {
        h.insert(header::SET_COOKIE, c);
    }
    resp
}

async fn api(State(app): State<Arc<App>>, Path(method): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
    if !is_json(&headers) {
        return respond(&Err(ApiError::new(Code::BadRequest, "Content-Type must be application/json")), None);
    }
    let req = match parse_request(&body) {
        Ok(r) => r,
        Err(e) => return respond(&Err(e), None),
    };
    let token = session_token(&headers);
    match method.as_str() {
        "auth.login" => {
            let r = match validate(LOGIN, &req) {
                Ok(p) => app.auth.login(&app.ctx, p.str("password").unwrap_or_default()).await,
                Err(e) => Err(e),
            };
            return match r {
                Ok(t) => {
                    let secs = app.ctx.cfg.session_idle.as_secs();
                    let c = format!("{COOKIE}={t}; Path=/api; HttpOnly; SameSite=Strict; Max-Age={secs}");
                    respond(&Ok(json!({"token": t, "idle_s": secs})), Some(c))
                }
                Err(e) => respond(&Err(e), None),
            };
        }
        "auth.logout" => {
            if let Some(t) = &token {
                app.auth.logout(t);
            }
            let c = format!("{COOKIE}=; Path=/api; HttpOnly; SameSite=Strict; Max-Age=0");
            return respond(&Ok(json!({})), Some(c));
        }
        _ => {}
    }
    let session = token.is_some_and(|t| app.auth.check(&t));
    let grant = Grant { read: session || app.ctx.cfg.open_reads, write: session };
    let Ok(_slot) = tokio::time::timeout(Duration::from_secs(5), app.slots.acquire()).await else {
        return respond(&Err(ApiError::busy("the device is busy with other requests")), None);
    };
    let r = dispatch(&app.ctx, &method, &req, grant).await;
    respond(&r, None)
}

async fn fallback() -> Response {
    let mut r = respond(&Err(ApiError::new(Code::UnknownMethod, "POST /api/<method>")), None);
    *r.status_mut() = StatusCode::NOT_FOUND;
    r
}

async fn shutdown() {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut term), Ok(mut int)) = (signal(SignalKind::terminate()), signal(SignalKind::interrupt())) else {
        return std::future::pending().await;
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}

/// Serve until SIGTERM or SIGINT. Every address must bind, or nothing runs.
pub async fn serve(ctx: Ctx) -> Result<(), String> {
    let mut addrs: Vec<SocketAddr> = ctx.cfg.listen.clone();
    if ctx.cfg.lan {
        let ip = ctx.lan_ip().await;
        let port = addrs.first().map(|a| a.port()).unwrap_or(8300);
        let a: SocketAddr = format!("{ip}:{port}").parse().map_err(|_| format!("bad LAN address {ip}"))?;
        if !addrs.contains(&a) {
            addrs.push(a);
        }
    }
    let app = Arc::new(App { auth: Auth::new(ctx.cfg.session_idle), ctx, slots: Semaphore::new(CONCURRENCY) });
    let router = Router::new()
        .route("/api/{method}", post(api))
        .fallback(fallback)
        .layer(DefaultBodyLimit::max(REQUEST_MAX))
        .with_state(app);
    let mut listeners = Vec::new();
    for a in &addrs {
        let l = tokio::net::TcpListener::bind(a).await.map_err(|e| format!("cannot listen on {a}: {e}"))?;
        eprintln!("mu300d: listening on http://{a}/api/");
        listeners.push(l);
    }
    let mut tasks = tokio::task::JoinSet::new();
    for l in listeners {
        let r = router.clone();
        tasks.spawn(async move { axum::serve(l, r).with_graceful_shutdown(shutdown()).await });
    }
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(format!("server error: {e}")),
            Err(e) => return Err(format!("server task failed: {e}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_from_headers() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("a=b; mu300d_session=abc; c=d"));
        assert_eq!(session_token(&h).as_deref(), Some("abc"));
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer xyz"));
        assert_eq!(session_token(&h).as_deref(), Some("xyz"));
        assert!(!is_json(&h));
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json; charset=utf-8"));
        assert!(is_json(&h));
    }
}

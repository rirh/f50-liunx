//! HTTP sessions: the device password checked by mu300-pwcheck against the shadow file, a random token, sessions in
//! memory with idle expiry, and login throttling.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::api::error::{ApiError, ApiResult, Code};
use crate::exec::Tool;
use crate::sys::Ctx;

/// Failed logins allowed per `FAIL_WINDOW`, for everyone together.
const FAIL_MAX: usize = 5;
const FAIL_WINDOW: Duration = Duration::from_secs(60);
const SESSIONS_MAX: usize = 32;

pub struct Auth {
    idle: Duration,
    sessions: Mutex<HashMap<String, Instant>>,
    fails: Mutex<VecDeque<Instant>>,
    /// One login at a time, so parallel guesses do not get past the throttle.
    gate: tokio::sync::Mutex<()>,
}

fn token() -> ApiResult<String> {
    let mut b = [0u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|_| ApiError::failed("no random source"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// The account's hash from the shadow file; None when it has no usable password (empty, locked, missing).
pub fn shadow_hash(text: &str, user: &str) -> Option<String> {
    let line = text.lines().find(|l| l.split(':').next() == Some(user))?;
    let hash = line.split(':').nth(1)?;
    (!hash.is_empty() && !hash.starts_with('!') && !hash.starts_with('*')).then(|| hash.to_owned())
}

impl Auth {
    pub fn new(idle: Duration) -> Self {
        Auth {
            idle,
            sessions: Mutex::new(HashMap::new()),
            fails: Mutex::new(VecDeque::new()),
            gate: tokio::sync::Mutex::new(()),
        }
    }

    /// The token names a live session (its idle time starts again).
    pub fn check(&self, token: &str) -> bool {
        let mut s = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        s.retain(|_, last| now.duration_since(*last) < self.idle);
        match s.get_mut(token) {
            Some(last) => {
                *last = now;
                true
            }
            None => false,
        }
    }

    pub fn logout(&self, token: &str) {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).remove(token);
    }

    fn throttled(&self) -> bool {
        let mut f = self.fails.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        while f.front().is_some_and(|t| now.duration_since(*t) > FAIL_WINDOW) {
            f.pop_front();
        }
        f.len() >= FAIL_MAX
    }

    fn failed(&self) {
        self.fails.lock().unwrap_or_else(|e| e.into_inner()).push_back(Instant::now());
    }

    /// A new session for the device password, or `unauthorized` / `forbidden`.
    pub async fn login(&self, ctx: &Ctx, password: &str) -> ApiResult<String> {
        let _one = self.gate.lock().await;
        if self.throttled() {
            return Err(ApiError::new(Code::Forbidden, "too many failed logins; wait a minute"));
        }
        let user = ctx.sys.login_user();
        let shadow = fs::read_to_string(ctx.cfg.sys("/etc/shadow"))
            .map_err(|_| ApiError::failed("cannot read the password database"))?;
        let Some(hash) = shadow_hash(&shadow, user) else {
            return Err(ApiError::new(
                Code::Forbidden,
                format!("{user} has no password; set one first (system.password over adb)"),
            ));
        };
        let o = Tool::new(&ctx.cfg, &ctx.cfg.pwcheck)
            .stdin(format!("{password}\n{hash}\n"))
            .timeout_secs(10)
            .output()
            .await?;
        match o.rc {
            Some(0) => {}
            Some(1) => {
                self.failed();
                // a second per wrong guess, on top of the throttle
                tokio::time::sleep(Duration::from_secs(1)).await;
                return Err(ApiError::new(Code::Unauthorized, "wrong password"));
            }
            _ => return Err(ApiError::failed("the password could not be verified")),
        }
        let t = token()?;
        let mut s = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if s.len() >= SESSIONS_MAX
            && let Some(oldest) = s.iter().min_by_key(|(_, v)| **v).map(|(k, _)| k.clone())
        {
            s.remove(&oldest);
        }
        s.insert(t.clone(), Instant::now());
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow() {
        let t = "root:$6$x$y:19000:0:99999:7:::\nubuntu::19000::::::\nlocked:!$6$a:1::::::\n";
        assert_eq!(shadow_hash(t, "root").as_deref(), Some("$6$x$y"));
        assert_eq!(shadow_hash(t, "ubuntu"), None);
        assert_eq!(shadow_hash(t, "locked"), None);
        assert_eq!(shadow_hash(t, "nobody"), None);
        assert_eq!(shadow_hash(t, "roo"), None);
    }

    #[test]
    fn sessions_expire() {
        let a = Auth::new(Duration::from_millis(30));
        a.sessions.lock().unwrap().insert("t".into(), Instant::now());
        assert!(a.check("t"));
        assert!(!a.check("u"));
        std::thread::sleep(Duration::from_millis(50));
        assert!(!a.check("t"));
    }

    #[test]
    fn throttle() {
        let a = Auth::new(Duration::from_secs(60));
        for _ in 0..FAIL_MAX {
            assert!(!a.throttled());
            a.failed();
        }
        assert!(a.throttled());
    }
}

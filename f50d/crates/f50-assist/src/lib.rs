//! Helpers built on the other capabilities (`f50.balance`, `f50.remote`, `f50.ddns`):
//!
//! - `balance`: the SIM's balance, asked of the carrier by SMS (China Mobile `CXYE` to 10086 and the like) and read
//!   from its reply; on demand only (each query is a billable SMS), or from any reply that comes;
//! - `remote`: SMS remote control from allowed numbers (status, reboot, the proxy, Wi-Fi, the balance);
//! - `ddns`: Cloudflare DNS records kept at the router's (or a LAN device's) address.
//!
//! Like `f50.status`, nothing here reaches into a sibling crate: everything goes through the published methods
//! (`f50.sms send`, `f50.proxy set`, `f50.system reboot`...) on the same dispatch path a client uses, with the table
//! handed over once the API is assembled ([`Table`]). New messages are found by reading `f50.sms list_unread`.
//!
//! Settings live in `<state>/assist/*.json` (0600: they hold a DNS token and the remote's secret).

mod api;
mod balance;
mod curl;
mod ddns;
mod inbox;
mod remote;
mod store;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use f50_core::paths::Paths;
use f50_core::rpc::Object;
use f50_core::{Error, Result, fsx};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Every published object, set once the API is assembled.
pub type Table = Arc<OnceLock<Arc<Vec<Object>>>>;

#[derive(Debug)]
pub(crate) struct Inner {
    pub dir: PathBuf,
    pub table: Table,
    pub balance: Mutex<balance::State>,
    pub remote: Mutex<remote::State>,
    pub ddns: Mutex<ddns::State>,
    /// One balance query at a time.
    pub asking: tokio::sync::Mutex<()>,
}

impl Inner {
    /// One call through the published table.
    pub async fn call(&self, object: &str, method: &str, req: Value) -> Result<Value> {
        let table = self.table.get().ok_or_else(|| Error::busy("f50d is still starting"))?.clone();
        f50_core::rpc::dispatch(&table, object, method, &req).await
    }

    pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The helpers. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Assist {
    inner: Arc<Inner>,
}

impl Assist {
    /// Reads the settings and starts the inbox watcher and DDNS on `tracker`.
    pub fn start(paths: &Paths, table: Table, tracker: &TaskTracker, shutdown: CancellationToken) -> Result<Assist> {
        let dir = paths.state().join("assist");
        fsx::private_dir(&dir)?;
        let inner = Arc::new(Inner {
            balance: Mutex::new(balance::State::load(&dir)),
            remote: Mutex::new(remote::State::load(&dir)),
            ddns: Mutex::new(ddns::State::load(&dir)),
            dir,
            table,
            asking: tokio::sync::Mutex::new(()),
        });
        tracker.spawn(inbox::run(inner.clone(), shutdown.clone()));
        tracker.spawn(ddns::run(inner.clone(), shutdown));
        Ok(Assist { inner })
    }

    pub fn objects(&self) -> Vec<Object> {
        api::objects(&self.inner)
    }
}

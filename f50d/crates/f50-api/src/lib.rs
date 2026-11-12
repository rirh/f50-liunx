//! Every capability's objects assembled into the one API f50d publishes.
//!
//! - [`Api::new`] takes the started services and collects their objects, the job object and the AT terminal;
//! - [`status`]: `f50.status`, the dashboard in one call, built by calling the other objects' read methods through
//!   the same dispatch path a client uses (so it never reaches into a crate's internals);
//! - [`meta`]: `f50.meta`, the method table itself (objects, methods, access, fields and their rules), for a
//!   frontend to discover the API and for the generated reference;
//! - [`acl`]: the rpcd ACL generated from the same table, so access can never drift from the code.
//!
//! Objects that are only for the device itself (netifd's protocol handler, hotplug) are kept apart in
//! [`Api::internal`] and never appear in an ACL: uhttpd refuses them to every login.

pub mod acl;
pub mod meta;
pub mod status;

use std::sync::Arc;

use f50_at::At;
use f50_core::jobs::Jobs;
use f50_core::paths::Paths;
use f50_core::rpc::Object;
use serde_json::Value;

/// The started services the API is made of.
#[derive(Debug, Clone)]
pub struct Services {
    pub at: At,
    pub jobs: Jobs,
    pub paths: Paths,
    pub modem: f50_modem::Modem,
    pub data: f50_data::Data,
    pub sms: f50_sms::Sms,
    pub net: f50_net::Net,
    pub system: f50_system::System,
    pub hw: f50_hw::Hw,
    pub update: f50_update::Update,
}

/// The published API: the public objects (in the ACL) and the internal ones (never in it).
#[derive(Debug, Clone)]
pub struct Api {
    public: Arc<Vec<Object>>,
    internal: Arc<Vec<Object>>,
    all: Arc<Vec<Object>>,
}

impl Api {
    /// `internal`: objects for the device's own scripts (netifd's `f50.proto`, hotplug), never granted to a login.
    pub fn new(s: &Services, internal: Vec<Object>) -> Api {
        let mut public = Vec::new();
        public.extend(s.modem.objects());
        public.extend(s.data.objects());
        public.extend(s.sms.objects());
        public.extend(s.net.objects());
        public.extend(s.system.objects());
        public.extend(s.hw.objects());
        public.extend(s.update.objects());
        public.push(s.jobs.object());
        public.push(f50_at::terminal::object(s.at.clone(), s.jobs.clone(), &s.paths));
        let described = public.clone();
        public.push(meta::object(described));
        let snapshot = Arc::new(public.clone());
        public.push(status::object(snapshot));
        let all = public.iter().chain(internal.iter()).cloned().collect();
        Api { public: Arc::new(public), internal: Arc::new(internal), all: Arc::new(all) }
    }

    pub fn public(&self) -> &[Object] {
        &self.public
    }

    pub fn internal(&self) -> &[Object] {
        &self.internal
    }

    /// Every object, public first: what the ubus server registers.
    pub fn all(&self) -> &[Object] {
        &self.all
    }

    /// One call, through validation, into the envelope.
    pub async fn call(&self, object: &str, method: &str, req: &Value) -> Value {
        f50_core::rpc::envelope(&f50_core::rpc::dispatch(&self.all, object, method, req).await)
    }

    /// The rpcd ACL for the public objects.
    pub fn acl(&self) -> Value {
        acl::generate(&self.public)
    }
}

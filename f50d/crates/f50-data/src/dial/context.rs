//! The data context and its credentials (`define_context`, FINDINGS 65 and 231).
//!
//! Credentials go to the modem as `AT+CGPCO=0,"<user>","<password>",<cid>,<0 none|1 PAP|2 CHAP|3 either>` after
//! `AT+CGDCONT` and before the context is activated, where ZTE's RIL sends them (read out of the B09 RIL by kanoqwq;
//! not measured against an APN that checks them). What holds for them:
//!
//! - They reach the modem through `At::send` only: never a command line, a log line, an error message or a reply.
//! - Both are printable ASCII without a quote or a backslash ([`crate::settings::credential_ok`]).
//! - `/etc/config/network`, which holds them, is root's alone before they are used, or there is no dial.
//! - Nothing falls back to a dial without them: unusable credentials, a modem that does not answer OK, end the dial.
//! - A mark per card (`/etc/f50d/apn-auth.<slot>`) says the modem was given credentials. It is written before they
//!   are sent and removed only once the modem took the empty form, so credentials taken out of the settings (or an
//!   APN changed to one without any) are taken out of the modem before the next dial; a modem that will not let go
//!   of them gets no dial rather than one that presents them to another network.
//! - ICCID binding: settings with an `iccid` are for that card alone; another card dials with none of them (its own
//!   context; earlier credentials taken back), and a card that does not say its ICCID gets no dial.

use std::path::PathBuf;
use std::time::Duration;

use f50_at::{At, Reply, SendOpts};
use f50_core::{Error, Result, fsx};
use f50_modem::Slot;
use f50_platform::Platform;

use crate::settings::{Auth, CardSettings, Credentials, apn_ok, settings_private};

const T: Duration = Duration::from_secs(5);

pub(crate) struct Context<'a> {
    pub at: &'a At,
    pub platform: &'a Platform,
    pub slot: Slot,
    pub cid: u8,
}

impl Context<'_> {
    fn mark(&self) -> PathBuf {
        self.platform.paths().state().join(format!("apn-auth.{}", self.slot.number()))
    }

    /// The shell's mark for the same card (`/etc/f50/apn-auth-set.sim<0 tray|1 internal>`), honoured after an
    /// upgrade so credentials it gave the modem are taken back too.
    fn old_mark(&self) -> PathBuf {
        self.platform.paths().sys(format!("/etc/f50/apn-auth-set.sim{}", self.slot.index()))
    }

    fn marked(&self) -> bool {
        let there = |p: PathBuf| std::fs::symlink_metadata(p).is_ok();
        there(self.mark()) || there(self.old_mark())
    }

    fn mark_set(&self) -> Result<()> {
        fsx::private_dir(&self.platform.paths().state())?;
        fsx::write_atomic(&self.mark(), b"", 0o600)
    }

    fn mark_clear(&self) {
        let _ = std::fs::remove_file(self.mark());
        let _ = std::fs::remove_file(self.old_mark());
    }

    async fn send(&self, cmd: &str) -> Result<Reply> {
        self.at.send(cmd, SendOpts::control(T)).await
    }

    /// The empty form, which takes credentials back. No answer ends the dial; another answer than OK too when the
    /// modem holds credentials of ours (the internal card's empty form was always sent and an ERROR ignored).
    async fn auth_clear(&self) -> Result<()> {
        let r = self.send(&format!("AT+CGPCO=0,\"\",\"\",{},0", self.cid)).await?;
        if r.is_ok() {
            self.mark_clear();
            return Ok(());
        }
        if self.marked() {
            return Err(Error::failed(
                "the modem did not take back the earlier APN credentials (AT+CGPCO): not dialling",
            ));
        }
        Ok(())
    }

    /// The credentials (or the empty form) for the context, after its AT+CGDCONT.
    async fn auth_send(&self, c: &Credentials) -> Result<()> {
        if c.auth == Auth::None {
            return self.auth_clear().await;
        }
        self.mark_set()
            .map_err(|_| Error::failed("cannot write the APN credentials mark: not giving them to the modem"))?;
        let cmd = format!("AT+CGPCO=0,\"{}\",\"{}\",{},{}", c.username, c.password, self.cid, c.auth as u8);
        let r = self.send(&cmd).await.map_err(|_| Error::failed("the modem did not answer the APN credentials"))?;
        drop(cmd);
        if r.is_ok() { Ok(()) } else { Err(Error::failed("the modem refused the APN credentials (AT+CGPCO)")) }
    }

    /// Is the card in use the one the settings are bound to? `Ok(false)`: another card.
    async fn same_card(&self, want: &str) -> Result<bool> {
        let want = want.to_ascii_uppercase();
        if !want.chars().all(|c| c.is_ascii_alphanumeric()) || !(18..=22).contains(&want.len()) {
            return Err(Error::failed("the ICCID of the APN settings is not 18 to 22 letters and digits"));
        }
        // +CCID: 8990..., quoted on some firmware; alphanumeric on some cards (FINDINGS 174)
        let have = self
            .send("AT+CCID")
            .await
            .ok()
            .and_then(|r| r.value("+CCID").map(|v| v.replace(['"', ' '], "").to_ascii_uppercase()))
            .filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric()))
            .ok_or_else(|| {
                Error::failed(
                    "the SIM card did not say its ICCID: the APN settings are for one card only, not dialling",
                )
            })?;
        Ok(have == want)
    }

    /// The context as the settings ask for it. With no APN nothing is defined: the modem keeps the context the SIM
    /// defines (and credentials given earlier are taken back all the same).
    pub async fn define(&self, settings: &CardSettings) -> Result<()> {
        let mut s = settings.clone();
        if let Some(iccid) = &settings.iccid
            && !self.same_card(iccid).await?
        {
            tracing::info!("the APN settings are for another SIM card: dialling with this card's own context");
            s = CardSettings { roaming_raw: s.roaming_raw, ..CardSettings::default() };
        }
        if !apn_ok(&s.apn) {
            return Err(Error::failed(
                "the APN contains a quote, a backslash or a line break: not defining the context",
            ));
        }
        let creds = s.credentials()?;
        if creds.auth != Auth::None {
            if s.apn.is_empty() {
                return Err(Error::failed(
                    "an APN user name or password is set without an APN: set the APN, or take them out",
                ));
            }
            let conf = self.platform.paths().sys("/etc/config/network");
            if !settings_private(&conf) {
                return Err(Error::failed(
                    "/etc/config/network holds the APN password and others can read it (chmod 600 it): not dialling",
                ));
            }
        }
        if s.apn.is_empty() {
            if self.marked() {
                self.auth_clear().await?;
            }
            return Ok(());
        }
        let pdp = s.pdp_or_default(self.slot).at_word();
        // an ERROR here was never fatal (the shell's AT client exited 0 on it): logged, and the address check
        // after the activation decides
        let ok = |r: Reply| {
            if !r.is_ok() {
                tracing::warn!(status = ?r.status, "AT+CGDCONT was not answered OK");
            }
        };
        match self.slot {
            // The internal card: the stock RIL's definition, byte for byte (dual stack with the extended context, PCO
            // and QoS fields).
            Slot::Two => {
                self.send("AT+CDEFMP=1,\"0\"").await?;
                let cmd = format!("AT+CGDCONT={},\"{pdp}\",\"{}\",\"\",0,0,0,0,1,,,,,,,,,,,,,0", self.cid, s.apn);
                ok(self.send(&cmd).await?);
                self.auth_send(&creds).await?;
                self.send("AT+CGEQREQ=1,2,0,0,0,0,2,0,\"1e4\",\"0e0\",3,0,0").await?;
            }
            // The tray card: the plain definition it was always given; credentials only when there are some to give or
            // to take back.
            Slot::One => {
                ok(self.send(&format!("AT+CGDCONT={},\"{pdp}\",\"{}\"", self.cid, s.apn)).await?);
                if creds.auth != Auth::None || self.marked() {
                    self.auth_send(&creds).await?;
                }
            }
        }
        Ok(())
    }
}

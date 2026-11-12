//! Jobs: they outlive the call that started it, keep secrets off argv, exclude their own kind, honour cancel and
//! timeouts.

mod common;

use std::fs;

use common::{Dev, wait_until};

fn job_id(r: &common::Reply) -> String {
    assert_eq!(r.json["ok"], true, "{}", r.json);
    r.json["data"]["job"].as_str().expect("a job id").to_owned()
}

#[test]
fn join_runs_detached_with_the_password_on_stdin() {
    let d = Dev::new();
    d.bin("wifi-client", &format!("{}; sleep 1; echo joined", d.save_stdin("wifi-client")));
    let r = d.call("wificlient.join", "{\"ssid\":\"Cafe\",\"password\":\"hunter22\"}");
    let id = job_id(&r);
    assert!(id.starts_with("wificlient-join-"), "{id}");
    // the call has returned; the job goes on
    let st = d.call("job.status", &format!("{{\"id\":\"{id}\"}}"));
    assert_eq!(st.json["data"]["state"], "running", "{}", st.json);
    let done = d.wait_job(&id, 15);
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["result"]["rc"], 0);
    assert!(done["result"]["text"].as_str().unwrap().contains("joined"));
    assert!(done["finished"].as_u64().is_some());
    assert_eq!(d.stdin_of("wifi-client"), "hunter22\n");
    assert!(d.calls().contains("wifi-client connect Cafe -"));
    assert!(!d.calls().contains("hunter22"));
    // the secret file is gone from the job directory
    assert!(!d.p(&format!("run/jobs/{id}/stdin")).exists());
    // the ssid may not look like an option, and a password is needed unless open
    assert_eq!(d.call("wificlient.join", "{\"ssid\":\"-x\",\"open\":true}").json["code"], "bad-value");
    assert_eq!(d.call("wificlient.join", "{\"ssid\":\"x\"}").json["code"], "missing");
    let list = d.call("job.list", "{}");
    assert_eq!(list.json["data"]["jobs"][0]["id"], id.as_str());
}

#[test]
fn one_job_per_kind_and_cancel() {
    let d = Dev::new();
    d.bin("wifi-client", "trap 'echo got-term; exit 143' TERM; echo scanning; sleep 30 & wait");
    let id = job_id(&d.call("wificlient.scan", "{}"));
    let r = d.call("wificlient.leave", "{}");
    assert_eq!(r.json["code"], "busy", "{}", r.json);
    assert!(wait_until(5, || d.calls().contains("wifi-client scan")));
    let r = d.call("job.cancel", &format!("{{\"id\":\"{id}\"}}"));
    assert_eq!(r.json["ok"], true, "{}", r.json);
    let st = d.wait_job(&id, 15);
    assert_eq!(st["state"], "cancelled", "{st}");
    // another kind is not blocked, and the kind is free again
    d.bin("wifi-client", "exit 0");
    let id2 = job_id(&d.call("wificlient.leave", "{\"forget\":true}"));
    assert_eq!(d.wait_job(&id2, 15)["state"], "done");
    assert!(d.calls().contains("wifi-client forget"));
}

#[test]
fn failed_job_reports_the_tools_message() {
    let d = Dev::new();
    d.bin("mu300-update", "echo 'downloading'; echo 'mu300-update: no space left on /mnt/mu300-disk' >&2; exit 4");
    let id = job_id(&d.call("update.check", "{}"));
    let st = d.wait_job(&id, 15);
    assert_eq!(st["state"], "failed");
    assert_eq!(st["result"]["rc"], 4);
    assert_eq!(st["error"], "mu300-update: no space left on /mnt/mu300-disk");
}

#[test]
fn update_apply_cannot_be_cancelled() {
    let d = Dev::new();
    d.bin("mu300-update", "sleep 3");
    let id = job_id(&d.call("update.apply", "{}"));
    let r = d.call("job.cancel", &format!("{{\"id\":\"{id}\"}}"));
    assert_eq!(r.json["code"], "forbidden", "{}", r.json);
    assert_eq!(d.call("update.check", "{}").json["code"], "busy");
    assert_eq!(d.wait_job(&id, 15)["state"], "done");
}

#[test]
fn unknown_and_malformed_ids() {
    let d = Dev::new();
    assert_eq!(d.call("job.status", "{\"id\":\"nope-1\"}").json["code"], "not-found");
    assert_eq!(d.call("job.status", "{\"id\":\"../etc\"}").json["code"], "bad-value");
    assert_eq!(d.call("job.status", "{}").json["code"], "missing");
}

#[test]
fn sms_send_and_lock_set_are_jobs() {
    let d = Dev::new();
    d.bin("sms", &format!("{}; echo sent", d.save_stdin("sms")));
    let id = job_id(&d.call("sms.send", "{\"number\":\"+905551112233\",\"text\":\"Merhaba\\nsatır 2\"}"));
    assert_eq!(d.wait_job(&id, 15)["state"], "done");
    assert_eq!(d.stdin_of("sms"), "+905551112233\nMerhaba\nsatır 2");
    assert!(d.calls().contains("sms send --stdin\n"));
    assert_eq!(d.call("sms.send", "{\"number\":\"-1\",\"text\":\"x\"}").json["code"], "bad-value");
    assert_eq!(d.call("sms.send", "{\"number\":\"1\",\"text\":\"\"}").json["code"], "bad-value");
    let long = "ş".repeat(481);
    assert_eq!(d.call("sms.send", &format!("{{\"number\":\"1\",\"text\":\"{long}\"}}")).json["code"], "bad-value");

    d.adapter("lock", "exit 0");
    let id = job_id(&d.call("lock.set", "{\"kind\":\"lte\",\"val\":\"1,3,20\"}"));
    assert_eq!(d.wait_job(&id, 15)["state"], "done");
    assert!(d.calls().contains("lock apply lte 1,3,20"));
    assert_eq!(d.call("lock.set", "{\"kind\":\"cell\",\"val\":\"nr:1\"}").json["code"], "bad-value");
    assert_eq!(d.call("lock.set", "{\"kind\":\"mode\"}").json["code"], "missing");
    let id = job_id(&d.call("lock.set", "{\"kind\":\"nr\"}"));
    assert_eq!(d.wait_job(&id, 15)["state"], "done");
    assert!(d.calls().contains("lock apply nr \n") || d.calls().contains("lock apply nr\n"), "{}", d.calls());
}

#[test]
fn at_scan_is_a_job() {
    let d = Dev::new();
    d.bin("mu300-at", &format!("{}; sleep 1; echo '+COPS: (2,\"Op\")'", d.save_stdin("mu300-at")));
    let r = d.call("at.send", "{\"command\":\"AT+COPS=?\"}");
    assert_eq!(r.json["field"], "confirm");
    let id = job_id(&d.call("at.send", "{\"command\":\"AT+COPS=?\",\"confirm\":true}"));
    // the channel is held: a plain command is busy meanwhile
    assert_eq!(d.call("at.send", "{\"command\":\"AT+CSQ\"}").json["code"], "busy");
    let st = d.wait_job(&id, 15);
    assert_eq!(st["state"], "done", "{st}");
    assert!(st["result"]["text"].as_str().unwrap().contains("+COPS: (2,\"Op\")"));
    assert!(d.calls().contains("mu300-at -t 99 -"));
}

#[test]
fn jobs_dir_is_private() {
    let d = Dev::new();
    d.bin("wifi-client", "exit 0");
    let id = job_id(&d.call("wificlient.leave", "{}"));
    d.wait_job(&id, 15);
    use std::os::unix::fs::PermissionsExt;
    let m = fs::metadata(d.p("run/jobs")).unwrap().permissions().mode() & 0o777;
    assert_eq!(m, 0o700);
}

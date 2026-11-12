//! The command line: `call`, `methods`, `serve`, and the two internal entry points (`job-run`, `collect-at`).

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use mu300d::api::error::{ApiError, Code};
use mu300d::api::registry::{self, Grant};
use mu300d::api::{REQUEST_MAX, envelope, parse_request};
use mu300d::config::{Config, parse_listen};
use mu300d::sys::Ctx;

const USAGE: &str = "\
usage: mu300d call METHOD < REQUEST   one method; REQUEST is one JSON object on stdin (or nothing: {})
       mu300d methods                 the method names, one per line
       mu300d serve [--listen ADDR:PORT]... [--lan] [--open-reads]
                                      the API over HTTP: POST /api/METHOD (default 127.0.0.1:8300)
exit status of call: 0 ok, 1 refused or failed, 2 usage";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime")
}

fn config() -> Result<Config, ExitCode> {
    Config::from_env().map_err(|e| {
        eprintln!("mu300d: {e}");
        ExitCode::from(2)
    })
}

fn call(method: &str) -> ExitCode {
    let cfg = match config() {
        Ok(c) => c,
        Err(c) => return c,
    };
    let mut body = Vec::new();
    let read = std::io::stdin().take(REQUEST_MAX as u64 + 1).read_to_end(&mut body);
    // SAFETY: geteuid(2) has no preconditions.
    let root = unsafe { libc::geteuid() } == 0;
    let grant = Grant { read: true, write: root || cfg.noroot };
    let ctx = Ctx::new(cfg);
    let r = match (read, parse_request(&body)) {
        (Err(_), _) => Err(ApiError::new(Code::BadRequest, "cannot read the request")),
        (_, Err(e)) => Err(e),
        (_, Ok(_)) if registry::HTTP_ONLY.iter().any(|(n, _)| *n == method) => {
            Err(ApiError::new(Code::Unsupported, format!("{method} is only for HTTP sessions")))
        }
        (_, Ok(req)) => runtime().block_on(registry::dispatch(&ctx, method, &req, grant)),
    };
    let r = r.map_err(|e| if e.code == Code::Unauthorized { ApiError::new(Code::Unauthorized, "not root") } else { e });
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", envelope(&r));
    if r.is_ok() { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn serve(args: &[String]) -> ExitCode {
    let mut cfg = match config() {
        Ok(c) => c,
        Err(c) => return c,
    };
    let mut listen = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--listen" => match it.next().map(|v| parse_listen(v)) {
                Some(Ok(mut l)) => listen.append(&mut l),
                _ => return usage(),
            },
            "--lan" => cfg.lan = true,
            "--open-reads" => cfg.open_reads = true,
            _ => return usage(),
        }
    }
    if !listen.is_empty() {
        cfg.listen = listen;
    }
    match runtime().block_on(mu300d::http::serve(Ctx::new(cfg))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mu300d: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("call") if args.len() == 2 => call(&args[1]),
        Some("methods") if args.len() == 1 => {
            for n in registry::names() {
                println!("{n}");
            }
            ExitCode::SUCCESS
        }
        Some("serve") => serve(&args[1..]),
        Some("job-run") if args.len() == 2 => {
            ExitCode::from(mu300d::jobs::run_job(&PathBuf::from(&args[1])).clamp(0, 255) as u8)
        }
        Some("collect-at") if args.len() == 1 => match config() {
            Ok(cfg) => ExitCode::from(runtime().block_on(mu300d::cache::collect_at(&cfg)).clamp(0, 255) as u8),
            Err(c) => c,
        },
        Some("--version") => {
            println!("mu300d {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => usage(),
    }
}

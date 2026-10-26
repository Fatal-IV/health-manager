//! Elevated, allow-listed fix runner.
//!
//! `fix-helper --fix <id> [--arg <value>] --result <path>`
//!
//! Exit codes: 0 success; 2 malformed command line, unknown fix id or
//! invalid argument (nothing is run); 3 the fix failed or the result file
//! could not be written. The result JSON `{ok, message, params, log}` is
//! written to `--result` on every outcome once a valid result path is known.

mod actions;
mod cli;
mod log;
mod repair;
mod winfs;

use actions::keys;
use health_core::fixes::{validate_arg, FixId};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

const EXIT_OK: i32 = 0;
const EXIT_INVALID: i32 = 2;
const EXIT_FAILED: i32 = 3;

#[derive(Serialize)]
struct FixResult {
    ok: bool,
    /// i18n key under `fix.result`.
    message: &'static str,
    params: BTreeMap<String, String>,
    log: String,
}

impl FixResult {
    fn fail(message: &'static str, log: Vec<String>) -> Self {
        FixResult {
            ok: false,
            message,
            params: BTreeMap::new(),
            log: log.join("\n"),
        }
    }
}

fn write_result(path: &Path, r: &FixResult) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(r).map_err(std::io::Error::other)?;
    let mut f = winfs::open_no_follow(path, true)?;
    f.write_all(&json)
}

/// Writes the result (a failed write turns the exit code into 3) and logs.
fn finish(path: &Path, fix: &str, arg: Option<&str>, r: FixResult, code: i32) -> i32 {
    let outcome = match code {
        EXIT_OK => "ok",
        EXIT_INVALID => "rejected",
        _ => "error",
    };
    log::append_log(&log::LogEntry {
        fix,
        arg,
        outcome,
        message: r.message,
    });
    match write_result(path, &r) {
        Ok(()) => code,
        Err(e) => {
            eprintln!("fix-helper: result file not written: {e}");
            EXIT_FAILED
        }
    }
}

fn real_main() -> i32 {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let req = match cli::parse_args(&args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("fix-helper: {}", e.reason);
            let r = FixResult::fail(keys::INVALID, vec![e.reason.to_string()]);
            return match e.result {
                Some(path) => finish(&path, "", None, r, EXIT_INVALID),
                None => {
                    log::append_log(&log::LogEntry {
                        fix: "",
                        arg: None,
                        outcome: "rejected",
                        message: keys::INVALID,
                    });
                    EXIT_INVALID
                }
            };
        }
    };
    let arg = req.arg.as_deref();
    let fix = match FixId::from_str(&req.fix) {
        Ok(f) => f,
        Err(e) => {
            let r = FixResult::fail(keys::INVALID, vec![e.to_string()]);
            return finish(&req.result, &req.fix, arg, r, EXIT_INVALID);
        }
    };
    if let Err(e) = validate_arg(fix, arg) {
        let r = FixResult::fail(keys::INVALID, vec![e.to_string()]);
        return finish(&req.result, &req.fix, arg, r, EXIT_INVALID);
    }
    let (r, code) = match actions::run(fix, arg) {
        Ok(rep) => (
            FixResult {
                ok: true,
                message: rep.message.unwrap_or(keys::OK),
                params: rep.params,
                log: rep.log.join("\n"),
            },
            EXIT_OK,
        ),
        Err(e) => {
            let code = if e.key == keys::INVALID {
                EXIT_INVALID
            } else {
                EXIT_FAILED
            };
            (FixResult::fail(e.key, e.log), code)
        }
    };
    finish(&req.result, fix.as_str(), arg, r, code)
}

fn main() {
    std::process::exit(real_main());
}

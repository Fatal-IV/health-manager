//! Helper launcher and fix flow: runs `fix-helper.exe` (elevated only when the
//! fix needs it), collects its JSON result and maps it to a `FixOutcome`.
//!
//! Security notes: the helper is only ever taken from next to the running
//! executable (never PATH); arguments are a fixed array (no shell, no
//! cmd.exe); the result file is a fresh random path in the temp dir and the
//! helper's JSON is treated as untrusted (size limit, key pattern, truncation).
#![allow(dead_code)] // some helpers are used by tests and the real launcher only

use health_core::fixes::{fix_plan, validate_arg, FixId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const K_OK: &str = "fix.result.ok";
pub const K_FAILED: &str = "fix.result.failed";
pub const K_INVALID_ARG: &str = "fix.result.invalid_argument";
pub const K_HELPER_MISSING: &str = "fix.result.helper_missing";
pub const K_TIMEOUT: &str = "fix.result.timeout";
pub const K_NOT_AVAILABLE: &str = "fix.result.not_available";

/// Keys produced by this module itself (helper-provided keys are only checked
/// against the pattern; the helper's own keys live in tr.json too).
pub const PRODUCED_KEYS: [&str; 6] = [
    K_OK,
    K_FAILED,
    K_INVALID_ARG,
    K_HELPER_MISSING,
    K_TIMEOUT,
    K_NOT_AVAILABLE,
];

/// Aşama 1 has no long jobs (RunSfcDism is not implemented yet): 10 minutes.
pub const HELPER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
const MAX_RESULT_BYTES: u64 = 64 * 1024;
const MAX_PARAM_LEN: usize = 256;
const MAX_PARAMS: usize = 16;
const MAX_DETAIL_LEN: usize = 4096;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FixOutcome {
    /// `message` is a stable i18n key; `params` fill its placeholders.
    Applied {
        message: String,
        #[serde(default)]
        params: BTreeMap<String, String>,
    },
    Declined,
    Failed {
        message: String,
        #[serde(default)]
        params: BTreeMap<String, String>,
        detail: String,
    },
}

fn failed(message: &str, detail: impl Into<String>) -> FixOutcome {
    FixOutcome::Failed {
        message: message.to_string(),
        params: BTreeMap::new(),
        detail: truncate(&detail.into(), MAX_DETAIL_LEN),
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct HelperArgs {
    pub fix: String,
    pub arg: Option<String>,
    pub result: PathBuf,
}

impl HelperArgs {
    /// The fixed argument array: `--fix <id> [--arg <value>] --result <path>`.
    pub fn argv(&self) -> Vec<String> {
        let mut v = vec!["--fix".to_string(), self.fix.clone()];
        if let Some(a) = &self.arg {
            v.push("--arg".into());
            v.push(a.clone());
        }
        v.push("--result".into());
        v.push(self.result.to_string_lossy().into_owned());
        v
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LaunchStatus {
    Exited(u32),
    TimedOut,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LaunchError {
    /// The user dismissed the UAC prompt (ERROR_CANCELLED, 1223).
    Cancelled,
    Other(String),
}

pub trait Launcher {
    fn run_helper(&self, args: &HelperArgs, elevate: bool) -> Result<LaunchStatus, LaunchError>;
}

/// Quotes one argument per the CommandLineToArgvW rules.
pub fn quote_windows_arg(a: &str) -> String {
    if !a.is_empty() && !a.chars().any(|c| matches!(c, ' ' | '\t' | '\n' | '\x0b' | '"')) {
        return a.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for c in a.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

/// Builds the single parameter string for ShellExecuteExW.
pub fn build_parameters(args: &HelperArgs) -> String {
    args.argv()
        .iter()
        .map(|a| quote_windows_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `^fix\.result\.[a-z_]+$`
pub fn is_valid_key(k: &str) -> bool {
    match k.strip_prefix("fix.result.") {
        Some(rest) => {
            !rest.is_empty() && rest.len() <= 64 && rest.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        }
        None => false,
    }
}

/// `fix-helper.exe` next to the running executable; debug builds also accept
/// a `target/debug` sibling. Never searches PATH.
pub fn locate_helper(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let sibling = dir.join("fix-helper.exe");
    if sibling.is_file() {
        return Some(sibling);
    }
    if cfg!(debug_assertions) {
        let dev = dir.parent()?.join("debug").join("fix-helper.exe");
        if dev.is_file() {
            return Some(dev);
        }
    }
    None
}

/// Fresh absolute, non-existing `<temp>\saglik-denetimi-<128-bit hex>.json`.
pub fn new_result_path() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir();
    let dir = if dir.is_absolute() {
        dir
    } else {
        std::path::absolute(&dir).map_err(|e| e.to_string())?
    };
    for _ in 0..8 {
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).map_err(|e| e.to_string())?;
        let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let p = dir.join(format!("saglik-denetimi-{hex}.json"));
        if !p.exists() {
            return Ok(p);
        }
    }
    Err("no free result path".into())
}

#[derive(Deserialize)]
struct HelperJson {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    message: String,
    #[serde(default)]
    params: BTreeMap<String, String>,
    #[serde(default)]
    log: String,
}

fn read_result(path: &Path) -> Result<HelperJson, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("result file unreadable: {e}"))?;
    let mut buf = Vec::new();
    f.take(MAX_RESULT_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("result file unreadable: {e}"))?;
    if buf.len() as u64 > MAX_RESULT_BYTES {
        return Err("result file too large".into());
    }
    let text = String::from_utf8(buf).map_err(|_| "result file is not UTF-8".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("result file is not valid JSON: {e}"))
}

fn clean_params(p: BTreeMap<String, String>) -> BTreeMap<String, String> {
    p.into_iter()
        .take(MAX_PARAMS)
        .map(|(k, v)| (truncate(&k, 64), truncate(&v, MAX_PARAM_LEN)))
        .collect()
}

fn checked_key(k: &str) -> String {
    if is_valid_key(k) {
        k.to_string()
    } else {
        K_FAILED.to_string()
    }
}

/// Launch logic against any `Launcher` (the real one is Windows-only).
pub fn apply_with(l: &dyn Launcher, fix: FixId, arg: Option<&str>) -> FixOutcome {
    if let Err(e) = validate_arg(fix, arg) {
        return failed(K_INVALID_ARG, e.to_string());
    }
    let needs_admin = fix_plan(fix, arg).needs_admin;
    let result = match new_result_path() {
        Ok(p) => p,
        Err(e) => return failed(K_FAILED, e),
    };
    let args = HelperArgs {
        fix: fix.as_str().to_string(),
        arg: arg.map(String::from),
        result: result.clone(),
    };
    let launched = l.run_helper(&args, needs_admin);
    let outcome = match launched {
        Err(LaunchError::Cancelled) => FixOutcome::Declined,
        Err(LaunchError::Other(e)) => failed(K_FAILED, e),
        Ok(LaunchStatus::TimedOut) => failed(K_TIMEOUT, "helper did not finish in time"),
        Ok(LaunchStatus::Exited(2)) => failed(K_INVALID_ARG, "helper rejected the request (exit 2)"),
        Ok(LaunchStatus::Exited(code)) => match read_result(&result) {
            Err(e) => failed(K_FAILED, format!("helper exit {code}: {e}")),
            Ok(j) if code == 0 && j.ok => FixOutcome::Applied {
                message: checked_key(&j.message),
                params: clean_params(j.params),
            },
            Ok(j) => FixOutcome::Failed {
                message: if code == 0 { K_FAILED.to_string() } else { checked_key(&j.message) },
                params: clean_params(j.params),
                detail: truncate(&format!("helper exit {code}\n{}", j.log), MAX_DETAIL_LEN),
            },
        },
    };
    let _ = std::fs::remove_file(&result);
    outcome
}

pub fn apply_fix(fix: FixId, arg: Option<&str>) -> FixOutcome {
    // Validate before touching the file system or the launcher.
    if let Err(e) = validate_arg(fix, arg) {
        return failed(K_INVALID_ARG, e.to_string());
    }
    let helper = std::env::current_exe().ok().and_then(|e| locate_helper(&e));
    match helper {
        Some(h) => apply_with(&real::RealLauncher { helper: h }, fix, arg),
        None => failed(K_HELPER_MISSING, "fix-helper.exe not found next to the application"),
    }
}

#[cfg(windows)]
mod real {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::time::Instant;
    use windows::core::{w, HRESULT, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const ERROR_CANCELLED: u32 = 1223;

    pub struct RealLauncher {
        pub helper: PathBuf,
    }

    impl Launcher for RealLauncher {
        fn run_helper(&self, args: &HelperArgs, elevate: bool) -> Result<LaunchStatus, LaunchError> {
            if elevate {
                self.run_elevated(args)
            } else {
                self.run_plain(args)
            }
        }
    }

    impl RealLauncher {
        /// Normal CreateProcess with a fixed argument array; no shell.
        fn run_plain(&self, args: &HelperArgs) -> Result<LaunchStatus, LaunchError> {
            let mut child = std::process::Command::new(&self.helper)
                .args(args.argv())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| LaunchError::Other(format!("spawn failed: {e}")))?;
            let start = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(st)) => return Ok(LaunchStatus::Exited(st.code().unwrap_or(1) as u32)),
                    Ok(None) if start.elapsed() >= HELPER_TIMEOUT => {
                        let _ = child.kill(); // our own, unelevated child
                        let _ = child.wait();
                        return Ok(LaunchStatus::TimedOut);
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                    Err(e) => return Err(LaunchError::Other(format!("wait failed: {e}"))),
                }
            }
        }

        /// ShellExecuteExW `runas`. An elevated process cannot be killed from
        /// here, so on timeout it is left running and TimedOut is returned.
        fn run_elevated(&self, args: &HelperArgs) -> Result<LaunchStatus, LaunchError> {
            let file = HSTRING::from(self.helper.as_os_str());
            let params = HSTRING::from(build_parameters(args));
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
                lpVerb: w!("runas"),
                lpFile: PCWSTR(file.as_ptr()),
                lpParameters: PCWSTR(params.as_ptr()),
                nShow: SW_HIDE.0,
                ..Default::default()
            };
            unsafe {
                if let Err(e) = ShellExecuteExW(&mut info) {
                    return Err(if e.code() == HRESULT::from_win32(ERROR_CANCELLED) {
                        LaunchError::Cancelled
                    } else {
                        LaunchError::Other(format!("ShellExecuteExW failed: {e}"))
                    });
                }
                let h = info.hProcess;
                if h.is_invalid() {
                    return Err(LaunchError::Other("no process handle returned".into()));
                }
                let ms = HELPER_TIMEOUT.as_millis() as u32;
                let r = WaitForSingleObject(h, ms);
                let out = if r == WAIT_OBJECT_0 {
                    let mut code = 0u32;
                    match GetExitCodeProcess(h, &mut code) {
                        Ok(()) => Ok(LaunchStatus::Exited(code)),
                        Err(e) => Err(LaunchError::Other(format!("exit code unavailable: {e}"))),
                    }
                } else if r == WAIT_TIMEOUT {
                    Ok(LaunchStatus::TimedOut)
                } else {
                    Err(LaunchError::Other("wait on helper failed".into()))
                };
                let _ = CloseHandle(h);
                out
            }
        }
    }
}

#[cfg(not(windows))]
mod real {
    use super::*;
    pub struct RealLauncher {
        pub helper: PathBuf,
    }
    impl Launcher for RealLauncher {
        fn run_helper(&self, _: &HelperArgs, _: bool) -> Result<LaunchStatus, LaunchError> {
            Err(LaunchError::Other("unsupported platform".into()))
        }
    }
}

#[cfg(test)]
mod tests;

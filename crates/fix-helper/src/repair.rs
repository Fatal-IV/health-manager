//! Machine-wide repairs: end a process, clear the Windows Update download
//! cache, reset the update components, turn Defender real-time protection
//! and the firewall back on, and run DISM + SFC.
//!
//! Every command is a fixed program from System32 with fixed arguments;
//! the only caller-supplied value (a pid) is parsed to a number and never
//! reaches a command line.

use crate::actions::Params;

/// Image names that must never be ended, compared case-insensitively.
const PROTECTED: [&str; 16] = [
    "system",
    "registry",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "svchost.exe",
    "dwm.exe",
    "explorer.exe",
    "fontdrvhost.exe",
    "sihost.exe",
    "ctfmon.exe",
    "msmpeng.exe",
    "fix-helper.exe",
];

pub fn is_protected_name(image: &str) -> bool {
    let base = image
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(image)
        .to_lowercase();
    PROTECTED.contains(&base.as_str())
}

/// Stops each service in the given order, then starts them in reverse. A
/// failed stop never blocks the start (the service may simply be stopped
/// already); a start that fails, other than "already running" (2), is the error.
#[cfg_attr(not(windows), allow(dead_code))]
fn restart_with(
    run: &mut dyn FnMut(&[&str]) -> Result<i32, &'static str>,
    services: &[&str],
) -> Result<(), &'static str> {
    use crate::actions::keys;
    for s in services {
        let _ = run(&["stop", s, "/y"]);
    }
    for s in services.iter().rev() {
        match run(&["start", s]) {
            Ok(0 | 2) => {}
            _ => return Err(keys::FAILED),
        }
    }
    Ok(())
}

/// A queued print job: `<id>.SPL` (data) or `<id>.SHD` (shadow). Bare
/// extension names (`.SPL`), no extension and anything longer (`x.spl.exe`) are not.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_spool_file(name: &std::ffi::OsStr) -> bool {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("spl") || e.eq_ignore_ascii_case("shd"))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::actions::{clean_dir_contents, keys, mb};
    use crate::winfs::final_path;
    use health_core::probe::windows::{system32_dir, windows_dir};
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Runs a System32 program with fixed arguments; returns its exit code.
    fn run_exe(rel: &str, args: &[&str], log: &mut Vec<String>) -> Result<i32, &'static str> {
        let exe: PathBuf = system32_dir().join(rel);
        match Command::new(&exe)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            Ok(o) => {
                let code = o.status.code().unwrap_or(-1);
                log.push(format!("{rel} {} -> exit {code}", args.join(" ")));
                Ok(code)
            }
            Err(e) => {
                log.push(format!("{rel}: {e}"));
                Err(keys::FAILED)
            }
        }
    }

    pub fn end_process(pid_arg: &str, log: &mut Vec<String>) -> Result<Params, &'static str> {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        };
        let pid: u32 = pid_arg.parse().map_err(|_| keys::INVALID)?;
        if pid <= 4 || pid == std::process::id() {
            log.push("refused: system or own process".into());
            return Err(keys::INVALID);
        }
        let h = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                false,
                pid,
            )
        }
        .map_err(|e| {
            log.push(format!("OpenProcess: {e}"));
            keys::ITEM_NOT_FOUND
        })?;
        let result = (|| {
            let mut session = 0u32;
            if unsafe { ProcessIdToSessionId(pid, &mut session) }.is_err() || session == 0 {
                log.push("refused: service-session process".into());
                return Err(keys::INVALID);
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            unsafe {
                QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
            }
            .map_err(|e| {
                log.push(format!("QueryFullProcessImageNameW: {e}"));
                keys::FAILED
            })?;
            let image = String::from_utf16_lossy(&buf[..len as usize]);
            if is_protected_name(&image) {
                log.push(format!("refused: protected process {image}"));
                return Err(keys::INVALID);
            }
            unsafe { TerminateProcess(h, 1) }.map_err(|e| {
                log.push(format!("TerminateProcess: {e}"));
                keys::FAILED
            })?;
            log.push(format!("ended {image} (pid {pid})"));
            Ok(Params::from([("target".into(), pid.to_string())]))
        })();
        let _ = unsafe { CloseHandle(h) };
        result
    }

    pub fn clean_update_leftovers(log: &mut Vec<String>) -> Result<Params, &'static str> {
        let dir = windows_dir().join("SoftwareDistribution").join("Download");
        let Ok(root) = final_path(&dir) else {
            log.push("SoftwareDistribution\\Download not found".into());
            return Err(keys::ITEM_NOT_FOUND);
        };
        let st = clean_dir_contents(&dir, &root);
        log.push(format!(
            "deleted {} files, {} folders ({} bytes); skipped {}",
            st.files, st.dirs, st.bytes, st.skipped
        ));
        Ok(Params::from([
            ("deleted".into(), st.files.to_string()),
            ("freed_mb".into(), mb(st.bytes)),
        ]))
    }

    pub fn reset_windows_update(log: &mut Vec<String>) -> Result<Params, &'static str> {
        const SERVICES: [&str; 3] = ["wuauserv", "bits", "cryptsvc"];
        for s in SERVICES {
            // Exit 2 = already stopped; not an error.
            run_exe("net.exe", &["stop", s], log)?;
        }
        let base = windows_dir().join("SoftwareDistribution");
        let old = windows_dir().join("SoftwareDistribution.old");
        let mut outcome = Ok(());
        if base.exists() {
            if old.exists() {
                if let Err(e) = std::fs::remove_dir_all(&old) {
                    log.push(format!("remove SoftwareDistribution.old: {e}"));
                    outcome = Err(keys::FAILED);
                }
            }
            if outcome.is_ok() {
                if let Err(e) = std::fs::rename(&base, &old) {
                    log.push(format!("rename SoftwareDistribution: {e}"));
                    outcome = Err(keys::FAILED);
                }
            }
        }
        // Always bring the services back, even when the rename failed.
        for s in SERVICES.iter().rev() {
            let _ = run_exe("net.exe", &["start", s], log);
        }
        outcome.map(|_| Params::new())
    }

    pub fn enable_defender_realtime(log: &mut Vec<String>) -> Result<Params, &'static str> {
        let code = run_exe(
            "WindowsPowerShell\\v1.0\\powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Set-MpPreference -DisableRealtimeMonitoring $false",
            ],
            log,
        )?;
        if code == 0 {
            Ok(Params::new())
        } else {
            Err(keys::FAILED)
        }
    }

    pub fn enable_firewall(log: &mut Vec<String>) -> Result<Params, &'static str> {
        let code = run_exe(
            "netsh.exe",
            &["advfirewall", "set", "allprofiles", "state", "on"],
            log,
        )?;
        if code == 0 {
            Ok(Params::new())
        } else {
            Err(keys::FAILED)
        }
    }

    pub fn flush_dns(log: &mut Vec<String>) -> Result<Params, &'static str> {
        let code = run_exe("ipconfig.exe", &["/flushdns"], log)?;
        if code == 0 {
            Ok(Params::new())
        } else {
            Err(keys::FAILED)
        }
    }

    pub fn renew_ip(log: &mut Vec<String>) -> Result<Params, &'static str> {
        let release = run_exe("ipconfig.exe", &["/release"], log)?;
        // Renew even when the release failed: the aim is a working lease.
        let renew = run_exe("ipconfig.exe", &["/renew"], log)?;
        if renew == 0 {
            Ok(Params::new())
        } else {
            log.push(format!("release exited {release}, renew exited {renew}"));
            Err(keys::FAILED)
        }
    }

    pub fn reset_winsock(log: &mut Vec<String>) -> Result<Params, &'static str> {
        let code = run_exe("netsh.exe", &["winsock", "reset"], log)?;
        if code == 0 {
            Ok(Params::from([("reboot_required".into(), "true".into())]))
        } else {
            Err(keys::FAILED)
        }
    }

    /// `services` are fixed names from this file, never caller input.
    fn restart_services(
        services: &[&str],
        log: &mut Vec<String>,
    ) -> Result<Params, &'static str> {
        restart_with(&mut |args| run_exe("net.exe", args, log), services)?;
        Ok(Params::new())
    }

    pub fn restart_audio_services(log: &mut Vec<String>) -> Result<Params, &'static str> {
        restart_services(&["Audiosrv", "AudioEndpointBuilder"], log)
    }

    pub fn restart_spooler(log: &mut Vec<String>) -> Result<Params, &'static str> {
        restart_services(&["Spooler"], log)
    }

    /// Stops the spooler, deletes only `.SPL`/`.SHD` files directly inside
    /// `spool\PRINTERS` (no folders, no links), and always starts it again.
    pub fn clear_print_queue(log: &mut Vec<String>) -> Result<Params, &'static str> {
        use crate::winfs::{delete_verified, is_reparse};
        let _ = run_exe("net.exe", &["stop", "Spooler", "/y"], log);
        let dir = system32_dir().join("spool").join("PRINTERS");
        let cleaned = match final_path(&dir) {
            Ok(root) => {
                let mut deleted = 0u32;
                for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let path = entry.path();
                    let Ok(md) = std::fs::symlink_metadata(&path) else {
                        continue;
                    };
                    if is_reparse(&md) || !md.is_file() || !is_spool_file(&entry.file_name()) {
                        continue;
                    }
                    if delete_verified(&path, &root).is_ok() {
                        deleted += 1;
                    }
                }
                log.push(format!("deleted {deleted} queued print files"));
                Ok(Params::from([("deleted".into(), deleted.to_string())]))
            }
            Err(_) => {
                log.push("spool\\PRINTERS not found".into());
                Err(keys::ITEM_NOT_FOUND)
            }
        };
        // Always bring the spooler back, even when the folder was missing.
        match run_exe("net.exe", &["start", "Spooler"], log) {
            Ok(0 | 2) => cleaned,
            _ => Err(keys::FAILED),
        }
    }

    pub fn restart_bluetooth(log: &mut Vec<String>) -> Result<Params, &'static str> {
        restart_services(&["bthserv"], log)
    }

    pub fn run_sfc_dism(log: &mut Vec<String>) -> Result<Params, &'static str> {
        let dism = run_exe(
            "dism.exe",
            &["/Online", "/Cleanup-Image", "/RestoreHealth"],
            log,
        )?;
        let sfc = run_exe("sfc.exe", &["/scannow"], log)?;
        // DISM: 0 ok. SFC: 0 no violations, 1 repaired; anything else failed.
        if dism == 0 && (sfc == 0 || sfc == 1) {
            Ok(Params::from([("repaired".into(), (sfc == 1).to_string())]))
        } else {
            Err(keys::FAILED)
        }
    }
}

#[cfg(windows)]
pub use imp::*;

#[cfg(not(windows))]
mod stub {
    use super::*;
    use crate::actions::keys;
    fn na(log: &mut Vec<String>) -> Result<Params, &'static str> {
        log.push("unsupported platform".into());
        Err(keys::NOT_AVAILABLE)
    }
    pub fn end_process(_: &str, l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn clean_update_leftovers(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn reset_windows_update(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn enable_defender_realtime(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn enable_firewall(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn run_sfc_dism(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn flush_dns(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn renew_ip(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn reset_winsock(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn restart_audio_services(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn restart_spooler(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn clear_print_queue(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
    pub fn restart_bluetooth(l: &mut Vec<String>) -> Result<Params, &'static str> {
        na(l)
    }
}
#[cfg(not(windows))]
pub use stub::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded(
        services: &[&str],
        mut answer: impl FnMut(&[&str]) -> Result<i32, &'static str>,
    ) -> (Result<(), &'static str>, Vec<Vec<String>>) {
        let mut calls: Vec<Vec<String>> = Vec::new();
        let r = restart_with(
            &mut |args| {
                calls.push(args.iter().map(|a| a.to_string()).collect());
                answer(args)
            },
            services,
        );
        (r, calls)
    }

    #[test]
    fn restart_with_stops_in_order_then_starts_in_reverse() {
        let (r, calls) = recorded(&["A", "B"], |_| Ok(0));
        assert_eq!(r, Ok(()));
        assert_eq!(
            calls,
            vec![
                vec!["stop", "A", "/y"],
                vec!["stop", "B", "/y"],
                vec!["start", "B"],
                vec!["start", "A"],
            ]
        );
    }

    #[test]
    fn restart_with_ignores_stop_errors_and_still_starts() {
        let (r, calls) = recorded(&["A", "B"], |args| {
            if args[0] == "stop" {
                Err(crate::actions::keys::FAILED)
            } else {
                Ok(0)
            }
        });
        assert_eq!(r, Ok(()));
        assert_eq!(calls.len(), 4);
    }

    #[test]
    fn restart_with_accepts_start_exit_2_and_fails_on_other_codes() {
        let start_with = |code| {
            recorded(&["A"], move |args| Ok(if args[0] == "start" { code } else { 0 })).0
        };
        assert_eq!(start_with(2), Ok(()));
        assert_eq!(start_with(1060), Err(crate::actions::keys::FAILED));
        let erroring = recorded(&["A"], |args| {
            if args[0] == "start" { Err(crate::actions::keys::FAILED) } else { Ok(0) }
        })
        .0;
        assert_eq!(erroring, Err(crate::actions::keys::FAILED));
    }

    #[test]
    fn is_spool_file_matches_only_spl_and_shd() {
        use std::ffi::OsStr;
        for ok in ["00001.SPL", "00001.shd", "00002.Spl"] {
            assert!(is_spool_file(OsStr::new(ok)), "{ok}");
        }
        for no in ["x.txt", "x.spl.exe", "spl", ".SPL", "00001.SPLX", ""] {
            assert!(!is_spool_file(OsStr::new(no)), "{no:?}");
        }
    }

    #[test]
    fn protected_names_are_matched_by_basename() {
        for p in [
            r"C:\Windows\System32\lsass.exe",
            "SVCHOST.EXE",
            r"C:\x\Explorer.exe",
            "System",
        ] {
            assert!(is_protected_name(p), "{p}");
        }
        for ok in [
            r"C:\Program Files\App\notepad.exe",
            "chrome.exe",
            "lsass.exe.bak",
        ] {
            assert!(!is_protected_name(ok), "{ok}");
        }
    }
}

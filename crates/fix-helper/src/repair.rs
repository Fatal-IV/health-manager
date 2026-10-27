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
}
#[cfg(not(windows))]
pub use stub::*;

#[cfg(test)]
mod tests {
    use super::*;

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

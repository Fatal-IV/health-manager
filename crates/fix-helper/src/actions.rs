//! Allow-listed fixes. The user-level ones live here; the machine-wide
//! repairs are in `repair.rs`.
//!
//! Arguments are DATA only: they are compared against freshly enumerated
//! system state and never become a path, registry key or command line.

use health_core::fixes::{validate_arg, FixId};
use health_core::probe::RunEntry;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// i18n keys of the result `message` (Turkish strings in `ui/src/i18n/tr.json`).
pub mod keys {
    pub const OK: &str = "fix.result.ok";
    pub const INVALID: &str = "fix.result.invalid";
    pub const NOT_ELEVATED: &str = "fix.result.not_elevated";
    pub const ITEM_NOT_FOUND: &str = "fix.result.item_not_found";
    pub const NOT_AVAILABLE: &str = "fix.result.not_available";
    pub const FAILED: &str = "fix.result.failed";
    pub const TEMP_UNSUPPORTED: &str = "fix.result.temp_unsupported";
    pub const BOOT_READ: &str = "fix.result.boot_read";
    pub const BOOT_NOT_FOUND: &str = "fix.result.boot_not_found";
    /// Every key the helper can produce (checked against tr.json in tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub const PRODUCED: [&str; 9] = [
        OK,
        INVALID,
        NOT_ELEVATED,
        ITEM_NOT_FOUND,
        NOT_AVAILABLE,
        FAILED,
        TEMP_UNSUPPORTED,
        BOOT_READ,
        BOOT_NOT_FOUND,
    ];
}

pub type Params = BTreeMap<String, String>;

#[derive(Debug, Default)]
pub struct ActionReport {
    /// Result key when it is not the generic `keys::OK`.
    pub message: Option<&'static str>,
    pub params: Params,
    /// Technical detail lines (English), shown under "Ayrıntılar".
    pub log: Vec<String>,
}

#[derive(Debug)]
pub struct ActionError {
    pub key: &'static str,
    pub log: Vec<String>,
}

pub fn run(fix: FixId, arg: Option<&str>) -> Result<ActionReport, ActionError> {
    // Defense in depth: main already validated, but this is the boundary.
    if let Err(e) = validate_arg(fix, arg) {
        return Err(ActionError {
            key: keys::INVALID,
            log: vec![e.to_string()],
        });
    }
    let mut log = Vec::new();
    let arg = arg.unwrap_or("");
    // Exhaustive on purpose: a new FixId must be placed here explicitly.
    let r = match fix {
        FixId::CleanTemp => clean_temp(&mut log),
        FixId::EmptyRecycleBin => empty_recycle_bin(&mut log),
        FixId::DisableStartupItem => disable_startup_item(arg, &mut log),
        FixId::EndProcess => crate::repair::end_process(arg, &mut log),
        FixId::CleanUpdateLeftovers => crate::repair::clean_update_leftovers(&mut log),
        FixId::ResetWindowsUpdate => crate::repair::reset_windows_update(&mut log),
        FixId::EnableDefenderRealtime => crate::repair::enable_defender_realtime(&mut log),
        FixId::EnableFirewall => crate::repair::enable_firewall(&mut log),
        FixId::RunSfcDism => crate::repair::run_sfc_dism(&mut log),
        FixId::FlushDns => crate::repair::flush_dns(&mut log),
        FixId::RenewIp => crate::repair::renew_ip(&mut log),
        FixId::ResetWinsock => crate::repair::reset_winsock(&mut log),
        FixId::ReadBootTime => read_boot_time(&mut log),
    };
    let message = (fix == FixId::ReadBootTime).then_some(keys::BOOT_READ);
    match r {
        Ok(params) => Ok(ActionReport { message, params, log }),
        Err(key) => Err(ActionError { key, log }),
    }
}

/// Reads the last boot duration from the event log. Needs administrator; changes nothing.
#[cfg(windows)]
fn read_boot_time(log: &mut Vec<String>) -> Result<Params, &'static str> {
    // The event log itself: the snapshot is what this very run would refresh.
    match health_core::probe::windows::boot_seconds_from_event_log() {
        Ok(Some(secs)) => {
            log.push(format!("last boot took {secs} s"));
            Ok(Params::from([("seconds".to_string(), secs.to_string())]))
        }
        Ok(None) => {
            log.push("no boot performance event found".into());
            Err(keys::BOOT_NOT_FOUND)
        }
        Err(e) => {
            log.push(format!("boot time unavailable: {e}"));
            Err(keys::FAILED)
        }
    }
}

#[cfg(not(windows))]
fn read_boot_time(log: &mut Vec<String>) -> Result<Params, &'static str> {
    log.push("unsupported platform".into());
    Err(keys::NOT_AVAILABLE)
}

/// Run entries whose id equals `id` exactly (byte for byte), one per
/// StartupApproved value to write.
pub fn find_run_matches<'a>(entries: &'a [RunEntry], id: &str) -> Vec<&'a RunEntry> {
    let mut out: Vec<&RunEntry> = Vec::new();
    for e in entries.iter().filter(|e| e.id() == id) {
        if !out
            .iter()
            .any(|o| o.hive == e.hive && o.approved_key == e.approved_key && o.name == e.name)
        {
            out.push(e);
        }
    }
    out
}

/// StartupApproved value meaning "disabled": `03 00 00 00` + FILETIME (UTC)
/// of the change, the format Task Manager writes. Re-enabling = writing
/// `02 00 00 00 ...` or deleting the value; the Run value is never touched.
pub fn disabled_approved_value(now: SystemTime) -> [u8; 12] {
    let d = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let ft = (d.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(d.subsec_nanos() / 100);
    let mut v = [0u8; 12];
    v[0] = 0x03;
    v[4..].copy_from_slice(&ft.to_le_bytes());
    v
}

/// Megabytes, rounded down, as a param string.
pub fn mb(bytes: u64) -> String {
    (bytes / (1024 * 1024)).to_string()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleanStats {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    /// Entries that could not be deleted (in use, access denied).
    pub skipped: u64,
    /// Links/junctions/reparse points left alone (never followed or deleted).
    pub links: u64,
}

#[cfg(windows)]
const MAX_DEPTH: usize = 64;

/// Deletes the CONTENTS of `dir` (never `dir` itself), best effort per
/// entry. Reparse points are skipped. Every deletion is verified through its
/// own handle against `root_final` (the root's final path).
#[cfg(windows)]
pub fn clean_dir_contents(dir: &std::path::Path, root_final: &str) -> CleanStats {
    let mut st = CleanStats::default();
    clean_rec(dir, root_final, 0, &mut st);
    st
}

#[cfg(windows)]
fn clean_rec(dir: &std::path::Path, root_final: &str, depth: usize, st: &mut CleanStats) {
    use crate::winfs::{delete_verified, is_reparse};
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(md) = std::fs::symlink_metadata(&path) else {
            st.skipped += 1;
            continue;
        };
        if is_reparse(&md) {
            st.links += 1;
            continue;
        }
        if md.is_dir() {
            if depth + 1 >= MAX_DEPTH {
                st.skipped += 1;
                continue;
            }
            clean_rec(&path, root_final, depth + 1, st);
            // Kept (not counted) when something inside could not be deleted.
            if delete_verified(&path, root_final).is_ok() {
                st.dirs += 1;
            }
        } else if delete_verified(&path, root_final).is_ok() {
            st.files += 1;
            st.bytes = st.bytes.saturating_add(md.len());
        } else {
            st.skipped += 1;
        }
    }
}

fn clean_params(st: CleanStats, log: &mut Vec<String>) -> Params {
    log.push(format!(
        "deleted {} files and {} folders ({} bytes); skipped {} in use, {} links",
        st.files, st.dirs, st.bytes, st.skipped, st.links
    ));
    Params::from([
        ("deleted".into(), st.files.to_string()),
        ("skipped".into(), st.skipped.to_string()),
        ("freed_mb".into(), mb(st.bytes)),
    ])
}

/// True only when the temp directory in use (`actual`, a final path) IS the
/// one allowed target (`expected`, the final path of
/// `<token profile>\AppData\Local\Temp`). Case-insensitive; a trailing
/// separator is ignored. A child, parent, sibling or other drive is refused.
pub fn temp_target_ok(actual: &str, expected: &str) -> bool {
    let norm = |s: &str| s.trim_end_matches('\\').to_lowercase();
    let (a, e) = (norm(actual), norm(expected));
    !e.is_empty() && a == e
}

/// The environment (`TMP`/`TEMP`) is not trusted: the only folder this fix
/// cleans is `<token profile>\AppData\Local\Temp` (the folder the scan
/// measures), and `temp_dir()` must resolve to exactly that. A custom temp
/// folder (e.g. `D:\Temp`) is refused with `temp_unsupported`.
#[cfg(windows)]
fn clean_temp(log: &mut Vec<String>) -> Result<Params, &'static str> {
    use crate::winfs::{final_path, user_profile_dir};
    let allowed = user_profile_dir()
        .map(|p| p.join("AppData").join("Local").join("Temp"))
        .and_then(|p| final_path(&p));
    let expected = allowed.map_err(|e| {
        log.push(format!("AppData\\Local\\Temp not resolvable: {e}"));
        keys::FAILED
    })?;
    let temp = std::env::temp_dir();
    let actual = final_path(&temp).unwrap_or_default();
    if !temp_target_ok(&actual, &expected) {
        log.push(format!(
            "refused: temp directory {} is not {expected}; custom temp folders are not cleaned",
            temp.display()
        ));
        return Err(keys::TEMP_UNSUPPORTED);
    }
    Ok(clean_params(clean_dir_contents(&temp, &expected), log))
}

#[cfg(windows)]
fn empty_recycle_bin(log: &mut Vec<String>) -> Result<Params, &'static str> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
        SHERB_NOSOUND, SHQUERYRBINFO,
    };
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    if let Err(e) = unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) } {
        log.push(format!("SHQueryRecycleBinW: {e}"));
        return Err(keys::FAILED);
    }
    let (items, bytes) = (info.i64NumItems.max(0), info.i64Size.max(0) as u64);
    if items > 0 {
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        if let Err(e) = unsafe { SHEmptyRecycleBinW(HWND::default(), PCWSTR::null(), flags) } {
            log.push(format!("SHEmptyRecycleBinW: {e}"));
            return Err(keys::FAILED);
        }
    }
    log.push(format!("emptied recycle bin: {items} items, {bytes} bytes"));
    Ok(Params::from([
        ("deleted".into(), items.to_string()),
        ("freed_mb".into(), mb(bytes)),
    ]))
}

#[cfg(windows)]
fn disable_startup_item(id: &str, log: &mut Vec<String>) -> Result<Params, &'static str> {
    use crate::winfs::{pcwstr, wide};
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SUCCESS, WIN32_ERROR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_SET_VALUE, KEY_WOW64_64KEY, REG_BINARY, REG_OPTION_NON_VOLATILE,
    };
    let fail = |rc: WIN32_ERROR| {
        if rc == ERROR_ACCESS_DENIED {
            keys::NOT_ELEVATED
        } else {
            keys::FAILED
        }
    };
    let entries = health_core::probe::windows::run_entries();
    let matches = find_run_matches(&entries, id);
    if matches.is_empty() {
        log.push("no Run entry has exactly this id".into());
        return Err(keys::ITEM_NOT_FOUND);
    }
    let value = disabled_approved_value(SystemTime::now());
    for m in matches {
        // Key and value name come from the enumeration, never from `id`.
        let root = if m.hive == "HKLM" {
            HKEY_LOCAL_MACHINE
        } else {
            HKEY_CURRENT_USER
        };
        let sub = wide(m.approved_key);
        let mut key = HKEY::default();
        let rc = unsafe {
            RegCreateKeyExW(
                root,
                pcwstr(&sub),
                0,
                windows::core::PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_WOW64_64KEY,
                None,
                &mut key,
                None,
            )
        };
        if rc != ERROR_SUCCESS {
            log.push(format!("open {}\\{}: {rc:?}", m.hive, m.approved_key));
            return Err(fail(rc));
        }
        let name = wide(&m.name);
        let rc = unsafe { RegSetValueExW(key, pcwstr(&name), 0, REG_BINARY, Some(&value)) };
        let _ = unsafe { RegCloseKey(key) };
        if rc != ERROR_SUCCESS {
            log.push(format!("set StartupApproved value: {rc:?}"));
            return Err(fail(rc));
        }
        log.push(format!(
            "disabled via {}\\{} (Run value kept, reversible)",
            m.hive, m.approved_key
        ));
    }
    Ok(Params::from([("target".into(), id.to_string())]))
}

#[cfg(not(windows))]
fn clean_temp(log: &mut Vec<String>) -> Result<Params, &'static str> {
    log.push("unsupported platform".into());
    Err(keys::NOT_AVAILABLE)
}

#[cfg(not(windows))]
fn empty_recycle_bin(log: &mut Vec<String>) -> Result<Params, &'static str> {
    clean_temp(log)
}

#[cfg(not(windows))]
fn disable_startup_item(_id: &str, log: &mut Vec<String>) -> Result<Params, &'static str> {
    clean_temp(log)
}

#[cfg(test)]
mod tests {
    use super::*;
    use health_core::probe::windows::{STARTUP_APPROVED_RUN, STARTUP_APPROVED_RUN32};

    fn entry(hive: &'static str, name: &str, approved_key: &'static str) -> RunEntry {
        RunEntry {
            hive,
            name: name.into(),
            approved_key,
            disabled: false,
        }
    }

    #[test]
    fn network_fixes_reject_an_argument_before_touching_anything() {
        for fix in [FixId::FlushDns, FixId::RenewIp, FixId::ResetWinsock] {
            let e = run(fix, Some("x")).unwrap_err();
            assert_eq!(e.key, keys::INVALID, "{fix:?}");
        }
    }

    #[test]
    fn run_id_matching_is_exact() {
        let entries = vec![
            entry("HKCU", "OneDrive", STARTUP_APPROVED_RUN),
            entry("HKCU", r"Foo\Bar", STARTUP_APPROVED_RUN),
            entry("HKCU", "Foo/Bar", STARTUP_APPROVED_RUN),
            entry("HKLM", "Adobe Updater", STARTUP_APPROVED_RUN),
            entry("HKLM", "Adobe Updater", STARTUP_APPROVED_RUN32),
            entry("HKLM", "Adobe Updater", STARTUP_APPROVED_RUN32),
        ];
        let names = |id: &str| -> Vec<(String, &'static str)> {
            find_run_matches(&entries, id)
                .iter()
                .map(|e| (e.id(), e.approved_key))
                .collect()
        };
        assert_eq!(
            names(r"HKCU|Foo\Bar"),
            vec![(r"HKCU|Foo\Bar".into(), STARTUP_APPROVED_RUN)]
        );
        assert_eq!(
            names("HKCU|Foo/Bar"),
            vec![("HKCU|Foo/Bar".into(), STARTUP_APPROVED_RUN)]
        );
        // Both views, duplicates collapsed.
        assert_eq!(names("HKLM|Adobe Updater").len(), 2);
        for miss in [
            "HKCU|onedrive",
            "HKLM|OneDrive",
            "HKCU|OneDrive ",
            "HKCU|One",
            "HKCU|Foo",
            r"HKCU|Foo\Bar\..",
            "HKCU|*",
            "",
        ] {
            assert!(names(miss).is_empty(), "{miss:?}");
        }
    }

    #[test]
    fn disabled_value_has_task_manager_format() {
        let v = disabled_approved_value(UNIX_EPOCH);
        assert_eq!(v[0], 0x03);
        assert_eq!(&v[1..4], &[0, 0, 0]);
        let ft = u64::from_le_bytes(v[4..].try_into().unwrap());
        assert_eq!(ft, 116_444_736_000_000_000);
        assert_eq!(v[0] & 1, 1, "odd first byte = disabled for the probe");
    }

    #[test]
    fn end_process_rejects_bad_pids_before_touching_anything() {
        assert_eq!(run(FixId::EndProcess, Some("x")).unwrap_err().key, keys::INVALID);
        assert_eq!(run(FixId::EndProcess, Some("4")).unwrap_err().key, keys::INVALID);
    }

    #[test]
    fn temp_guard_requires_the_exact_local_temp_folder() {
        let want = r"\\?\C:\Users\Ali\AppData\Local\Temp";
        assert!(temp_target_ok(want, want));
        assert!(temp_target_ok(r"\\?\c:\users\ali\appdata\local\TEMP", want));
        assert!(temp_target_ok(&format!("{want}\\"), want));
        for bad in [
            r"\\?\C:\Users\Ali\AppData\Local\Temp\sub",
            r"\\?\C:\Users\Ali\AppData\Local\Temp2",
            r"\\?\C:\Users\Ali\AppData\Local\Tem",
            r"\\?\C:\Users\Ali\Documents",
            r"\\?\C:\Users\Ali\Desktop",
            r"\\?\C:\Users\Ali",
            r"\\?\C:\",
            r"\\?\D:\Temp",
            "",
        ] {
            assert!(!temp_target_ok(bad, want), "{bad}");
        }
        assert!(!temp_target_ok("", ""));
    }

    #[test]
    fn every_message_key_exists_in_tr_json() {
        let tr: serde_json::Value =
            serde_json::from_str(include_str!("../../../ui/src/i18n/tr.json")).unwrap();
        let lookup = |key: &str| {
            let mut v = &tr;
            for part in key.split('.') {
                v = v.get(part).unwrap_or(&serde_json::Value::Null);
            }
            v.as_str().is_some_and(|s| !s.trim().is_empty())
        };
        for k in keys::PRODUCED.iter().chain(&["fix.result.declined"]) {
            assert!(lookup(k), "missing {k}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn clean_dir_contents_skips_reparse_points_and_counts() {
        use crate::winfs::final_path;
        let base = std::env::temp_dir().join(format!("fix-helper-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        let victim = base.join("victim");
        std::fs::create_dir_all(root.join("sub").join("deeper")).unwrap();
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::write(victim.join("keep.txt"), b"keep").unwrap();
        std::fs::write(root.join("a.txt"), [0u8; 10]).unwrap();
        std::fs::write(root.join("sub").join("b.txt"), [0u8; 20]).unwrap();
        std::fs::write(root.join("sub").join("deeper").join("c.bin"), [0u8; 5]).unwrap();
        let ro = root.join("readonly.txt");
        std::fs::write(&ro, [0u8; 7]).unwrap();
        let mut perm = std::fs::metadata(&ro).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&ro, perm).unwrap();
        // A file held open without delete sharing must be skipped.
        let locked_path = root.join("locked.txt");
        std::fs::write(&locked_path, [0u8; 3]).unwrap();
        use std::os::windows::fs::OpenOptionsExt;
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked_path)
            .unwrap();
        // Junction root/link -> victim (no privilege needed for /J).
        let cmd = health_core::probe::windows::system32_dir().join("cmd.exe");
        let junction = std::process::Command::new(cmd)
            .args(["/c", "mklink", "/J"])
            .arg(root.join("link"))
            .arg(&victim)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let root_final = final_path(&root).unwrap();
        let st = clean_dir_contents(&root, &root_final);
        drop(locked);

        assert_eq!(st.files, 4, "{st:?}"); // a, b, c, readonly
        assert_eq!(st.bytes, 42, "{st:?}");
        assert_eq!(st.dirs, 2, "{st:?}"); // sub, deeper
        assert_eq!(st.skipped, 1, "{st:?}"); // locked
        assert!(root.is_dir(), "root itself is kept");
        assert!(locked_path.exists());
        assert!(
            victim.join("keep.txt").exists(),
            "junction target untouched"
        );
        if junction {
            assert_eq!(st.links, 1);
            assert!(std::fs::symlink_metadata(root.join("link")).is_ok());
        } else {
            eprintln!("junction not creatable here; reparse case not exercised");
        }
        // A deletion outside the root is refused even when asked directly.
        assert!(crate::winfs::delete_verified(&victim.join("keep.txt"), &root_final).is_err());
        assert!(victim.join("keep.txt").exists());
        let _ = std::fs::remove_dir(root.join("link"));
        let _ = std::fs::remove_dir_all(&base);
    }
}

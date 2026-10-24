//! Append-only JSON-lines log at `%ProgramData%\SaglikDenetimi\logs\fix.log`.
//! Logging is best effort: it never panics and never fails a fix.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Rotate to `fix.log.1` once the log reaches this size.
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;
const MAX_ARG_CHARS: usize = 200;
const MAX_FIX_CHARS: usize = 64;

pub struct LogEntry<'a> {
    /// Fix id as requested (may be an unknown, hostile string).
    pub fix: &'a str,
    pub arg: Option<&'a str>,
    /// `ok`, `error` or `rejected`.
    pub outcome: &'a str,
    /// i18n message key.
    pub message: &'a str,
}

/// Escapes control and bidi/format characters (`\u{..}`) and truncates to
/// `max` characters, so a log line can neither be split nor visually spoofed.
pub fn sanitize(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        let bidi = matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}');
        if c.is_control() || bidi {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

/// `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub fn iso8601_utc(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// One JSON line (with trailing newline).
pub fn format_line(e: &LogEntry, now: SystemTime) -> String {
    let v = serde_json::json!({
        "ts": iso8601_utc(now),
        "fix": sanitize(e.fix, MAX_FIX_CHARS),
        "arg": e.arg.map(|a| sanitize(a, MAX_ARG_CHARS)),
        "outcome": e.outcome,
        "message": e.message,
    });
    format!("{v}\n")
}

/// The log directory. `SAGLIK_LOG_DIR` is honored ONLY in debug/test builds,
/// so a released elevated helper cannot be redirected through its environment.
pub fn log_dir() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(d) = std::env::var_os("SAGLIK_LOG_DIR") {
        return Some(PathBuf::from(d));
    }
    default_log_dir()
}

/// `ProgramData` from the known-folder API (not `%ProgramData%`).
#[cfg(windows)]
fn default_log_dir() -> Option<PathBuf> {
    crate::winfs::program_data().map(|p| p.join("SaglikDenetimi").join("logs"))
}

#[cfg(not(windows))]
fn default_log_dir() -> Option<PathBuf> {
    None
}

/// Appends `entry`; any failure is reported on stderr and otherwise ignored.
pub fn append_log(entry: &LogEntry) {
    let line = format_line(entry, SystemTime::now());
    let res = std::panic::catch_unwind(|| match log_dir() {
        Some(dir) => append_to(&dir, &line),
        None => Err(std::io::Error::other("no log directory")),
    });
    match res {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("fix-helper: log not written: {e}"),
        Err(_) => eprintln!("fix-helper: log not written (panic)"),
    }
}

pub fn append_to(dir: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("fix.log");
    if let Ok(md) = std::fs::symlink_metadata(&path) {
        if md.is_file() && md.len() >= MAX_LOG_BYTES {
            // rename/remove act on the directory entry itself, never on a link target.
            let old = dir.join("fix.log.1");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&path, &old)?;
        }
    }
    let mut f = crate::winfs::open_no_follow(&path, false)?;
    use std::io::Seek;
    f.seek(std::io::SeekFrom::End(0))?;
    f.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamps_are_utc_iso8601() {
        assert_eq!(iso8601_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = UNIX_EPOCH + Duration::from_secs(1_709_296_215);
        assert_eq!(iso8601_utc(t), "2024-03-01T12:30:15Z");
        let leap = UNIX_EPOCH + Duration::from_secs(1_709_164_800);
        assert_eq!(iso8601_utc(leap), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn args_are_escaped_and_truncated() {
        assert_eq!(sanitize("a\nb\u{0}c", 200), "a\\u{a}b\\u{0}c");
        assert_eq!(sanitize("x\u{202E}y", 200), "x\\u{202e}y");
        assert_eq!(sanitize("Yönetim", 200), "Yönetim");
        let long = "é".repeat(500);
        let s = sanitize(&long, 200);
        assert_eq!(s.chars().count(), 201);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn line_is_one_json_object() {
        let e = LogEntry {
            fix: "end_process",
            arg: Some("12\r\n{\"fake\":1}"),
            outcome: "error",
            message: "fix.result.process_gone",
        };
        let line = format_line(&e, UNIX_EPOCH);
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        let v: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(v["fix"], "end_process");
        assert_eq!(v["arg"], "12\\u{d}\\u{a}{\"fake\":1}");
        assert_eq!(v["outcome"], "error");
        assert_eq!(v["ts"], "1970-01-01T00:00:00Z");
        let none = LogEntry { arg: None, ..e };
        let v: serde_json::Value =
            serde_json::from_str(format_line(&none, UNIX_EPOCH).trim()).unwrap();
        assert!(v["arg"].is_null());
    }

    #[test]
    fn log_rotates_at_one_megabyte() {
        let dir = std::env::temp_dir().join(format!("fix-helper-rot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        append_to(&dir, "first\n").unwrap();
        std::fs::write(dir.join("fix.log"), vec![b'x'; MAX_LOG_BYTES as usize]).unwrap();
        append_to(&dir, "new\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("fix.log")).unwrap(),
            "new\n"
        );
        assert_eq!(
            std::fs::metadata(dir.join("fix.log.1")).unwrap().len(),
            MAX_LOG_BYTES
        );
        append_to(&dir, "more\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("fix.log")).unwrap(),
            "new\nmore\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_log_never_panics_on_a_bad_directory() {
        // A file where the directory should be: create_dir_all fails.
        let f = std::env::temp_dir().join(format!("fix-helper-notadir-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        assert!(append_to(&f.join("logs"), "x\n").is_err());
        let _ = std::fs::remove_file(&f);
    }
}

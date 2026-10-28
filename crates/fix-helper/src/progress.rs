//! Best-effort progress for the app: after each finished operation of a
//! multi-step repair one `{"op":n}` line is appended to `<result>.progress`.
//! The app creates that file before starting the helper; a missing or
//! unwritable file just means no progress. Progress never fails a repair.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static SINK: OnceLock<Mutex<File>> = OnceLock::new();

/// One progress line: operation `n` finished.
pub fn line(n: u32) -> String {
    format!("{{\"op\":{n}}}
")
}

fn progress_path(result: &Path) -> PathBuf {
    let mut p = result.as_os_str().to_owned();
    p.push(".progress");
    PathBuf::from(p)
}

/// Opens `<result>.progress` only if the app already created it, without
/// following a link, positioned at the end. Any problem means no progress.
fn open(result: &Path) -> Option<File> {
    let path = progress_path(result);
    std::fs::symlink_metadata(&path).ok()?;
    let mut f = crate::winfs::open_no_follow(&path, false).ok()?;
    f.seek(SeekFrom::End(0)).ok()?;
    Some(f)
}

fn append(f: &mut File, n: u32) {
    let _ = f.write_all(line(n).as_bytes());
}

/// Installs the progress file for this run (once); silently does nothing when it cannot.
pub fn init(result: &Path) {
    if let Some(f) = open(result) {
        let _ = SINK.set(Mutex::new(f));
    }
}

/// Operation `n` of the running repair finished.
pub fn done(n: u32) {
    if let Some(m) = SINK.get() {
        if let Ok(mut f) = m.lock() {
            append(&mut f, n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fix-helper-progress-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn line_is_one_json_object_per_line() {
        assert_eq!(line(2), "{\"op\":2}\n");
    }

    #[test]
    fn open_ignores_a_missing_file_and_does_not_create_it() {
        let dir = scratch("missing");
        let result = dir.join("r.json");
        assert!(open(&result).is_none());
        assert!(!dir.join("r.json.progress").exists());
    }

    #[test]
    fn done_appends_lines_in_order() {
        let dir = scratch("append");
        let result = dir.join("r.json");
        std::fs::write(dir.join("r.json.progress"), b"").unwrap();
        let mut f = open(&result).expect("existing file opens");
        append(&mut f, 1);
        append(&mut f, 2);
        let text = std::fs::read_to_string(dir.join("r.json.progress")).unwrap();
        assert_eq!(text, "{\"op\":1}\n{\"op\":2}\n");
    }

    #[test]
    fn done_without_init_does_nothing() {
        done(1); // no sink was installed in this test binary: must not panic
    }
}

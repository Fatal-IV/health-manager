//! The last measured boot time, left in a file next to the program.
//!
//! Windows only shows the boot duration to administrators. The installer
//! registers a startup task that runs `fix-helper.exe --boot-snapshot` with
//! administrator rights; it writes the value here. The app only reads this file
//! (which lives in a folder only administrators can write), so it never has to
//! ask for permission to show the boot time.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DIR: &str = "state";
pub const FILE: &str = "boot.json";
/// A boot that took more than a day is not a measurement.
const MAX_SECONDS: u32 = 86_400;
const MAX_FILE_BYTES: u64 = 1024;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub seconds: u32,
    pub measured_at_unix: u64,
}

/// `<folder of exe>\state\boot.json`
pub fn path_beside(exe: &Path) -> Option<PathBuf> {
    Some(exe.parent()?.join(DIR).join(FILE))
}

pub fn to_json(seconds: u32, now: SystemTime) -> String {
    let snap = Snapshot {
        seconds,
        measured_at_unix: now.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
    };
    serde_json::to_string(&snap).expect("snapshot serializes")
}

/// The seconds in a snapshot file; `None` for anything that is not a plausible reading.
pub fn parse(text: &str) -> Option<u32> {
    let snap: Snapshot = serde_json::from_str(text).ok()?;
    (1..=MAX_SECONDS).contains(&snap.seconds).then_some(snap.seconds)
}

/// The snapshot beside the running program, if there is a valid one.
pub fn read_beside_exe() -> Option<u32> {
    let path = path_beside(&std::env::current_exe().ok()?)?;
    if std::fs::metadata(&path).ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    parse(&std::fs::read_to_string(path).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn what_is_written_can_be_read_back() {
        let json = to_json(62, UNIX_EPOCH + Duration::from_secs(1_800_000_000));
        assert_eq!(parse(&json), Some(62));
        assert!(json.contains("1800000000"));
    }

    #[test]
    fn implausible_or_broken_files_are_not_a_reading() {
        assert_eq!(parse(r#"{"seconds":0,"measured_at_unix":1}"#), None);
        assert_eq!(parse(r#"{"seconds":90000,"measured_at_unix":1}"#), None);
        assert_eq!(parse(r#"{"seconds":"fast","measured_at_unix":1}"#), None);
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn the_file_lives_in_a_state_folder_next_to_the_program() {
        let p = path_beside(Path::new(r"C:\Program Files\Sağlık Denetimi\saglik-denetimi.exe")).unwrap();
        assert_eq!(p, Path::new(r"C:\Program Files\Sağlık Denetimi\state\boot.json"));
    }
}

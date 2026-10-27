//! `fix-helper.exe --boot-snapshot`: run by the startup task (as SYSTEM). Reads the boot
//! time from the event log and leaves it in `state\boot.json` next to this program, where
//! the app can read it without any permission prompt.
//!
//! This takes no arguments and writes to one place derived from its own location. It
//! refuses to run anywhere but under Program Files: a folder a normal user can write to
//! would let that user steer what a SYSTEM process writes.

use health_core::boot_snapshot;
use std::io::Write;
use std::path::Path;

const EXIT_OK: i32 = 0;
const EXIT_REFUSED: i32 = 2;
const EXIT_FAILED: i32 = 3;

/// True when `exe` is inside one of the Program Files folders.
pub fn in_program_files(exe: &Path, roots: &[String]) -> bool {
    let exe = exe.to_string_lossy().to_lowercase();
    roots.iter().filter(|r| !r.is_empty()).any(|r| {
        let root = r.trim_end_matches('\\').to_lowercase();
        exe.starts_with(&root) && exe[root.len()..].starts_with('\\')
    })
}

fn program_files() -> Vec<String> {
    ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .collect()
}

#[cfg(windows)]
fn measure() -> Option<u32> {
    health_core::probe::windows::boot_seconds_from_event_log().ok().flatten()
}

#[cfg(not(windows))]
fn measure() -> Option<u32> {
    None
}

pub fn run() -> i32 {
    let Ok(exe) = std::env::current_exe() else {
        return EXIT_FAILED;
    };
    if !in_program_files(&exe, &program_files()) {
        eprintln!("fix-helper: --boot-snapshot only runs from Program Files");
        return EXIT_REFUSED;
    }
    let Some(path) = boot_snapshot::path_beside(&exe) else {
        return EXIT_FAILED;
    };
    let Some(seconds) = measure() else {
        return EXIT_FAILED;
    };
    let json = boot_snapshot::to_json(seconds, std::time::SystemTime::now());
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().expect("path has a parent"))?;
        crate::winfs::open_no_follow(&path, true)?.write_all(json.as_bytes())
    };
    match write() {
        Ok(()) => EXIT_OK,
        Err(e) => {
            eprintln!("fix-helper: snapshot not written: {e}");
            EXIT_FAILED
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Vec<String> {
        vec![r"C:\Program Files".to_string(), r"C:\Program Files".to_string()]
    }

    #[test]
    fn only_paths_under_program_files_are_accepted() {
        assert!(in_program_files(Path::new(r"C:\Program Files\Sağlık Denetimi\fix-helper.exe"), &roots()));
        assert!(in_program_files(Path::new(r"c:\program files\x\fix-helper.exe"), &roots()));
        // A look-alike folder name and a user folder are not Program Files.
        assert!(!in_program_files(Path::new(r"C:\Program Files Evil\fix-helper.exe"), &roots()));
        assert!(!in_program_files(Path::new(r"C:\Users\x\AppData\Local\Programs\fix-helper.exe"), &roots()));
        assert!(!in_program_files(Path::new(r"C:\Program Files"), &roots()));
        assert!(!in_program_files(Path::new(r"C:\Program Files\x"), &[]));
        assert!(!in_program_files(Path::new(r"C:\Program Files\x"), &[String::new()]));
    }
}

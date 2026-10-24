//! Strict hand-rolled command line parser. Grammar (order free):
//! `--fix <id> [--arg <value>] --result <path>`; every flag at most once,
//! nothing else accepted. Values are kept as opaque data.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf, Prefix};

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    /// Raw fix id, not yet checked against the allow-list.
    pub fix: String,
    pub arg: Option<String>,
    pub result: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CliError {
    pub reason: &'static str,
    /// The result path, when exactly one valid `--result` was given, so the
    /// failure can still be reported there.
    pub result: Option<PathBuf>,
}

pub fn parse_args(args: &[OsString]) -> Result<Request, CliError> {
    let mut fix: Option<String> = None;
    let mut arg: Option<String> = None;
    let mut result: Option<String> = None;
    let mut error: Option<&'static str> = None;
    let mut dup_result = false;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let slot = match flag.to_str() {
            Some("--fix") => &mut fix,
            Some("--arg") => &mut arg,
            Some("--result") => &mut result,
            _ => {
                error.get_or_insert("unknown argument");
                continue;
            }
        };
        let Some(value) = it.next() else {
            error.get_or_insert("missing value");
            break;
        };
        let Some(value) = value.to_str() else {
            error.get_or_insert("value is not valid Unicode");
            continue;
        };
        if slot.is_some() {
            error.get_or_insert("duplicate flag");
            if flag == "--result" {
                dup_result = true;
            }
            continue;
        }
        *slot = Some(value.to_string());
    }
    let result_path = match (&result, dup_result) {
        (Some(r), false) => validate_result_path(Path::new(r))
            .map(|()| PathBuf::from(r))
            .map_err(|reason| {
                error.get_or_insert(reason);
            })
            .ok(),
        (None, _) => {
            error.get_or_insert("--result is required");
            None
        }
        (Some(_), true) => None,
    };
    let fail = |reason| CliError {
        reason,
        result: result_path.clone(),
    };
    if let Some(reason) = error {
        return Err(fail(reason));
    }
    let Some(fix) = fix else {
        return Err(fail("--fix is required"));
    };
    Ok(Request {
        fix,
        arg,
        result: result_path.expect("validated above"),
    })
}

/// DOS device names that must never be opened as a result file.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The result path must be a plain absolute drive path (`C:\...\x.json`): no
/// relative path, no UNC/device/verbatim namespace (`\\.\PhysicalDrive0`),
/// no `..`, no alternate data stream, no DOS device name, and not an
/// existing directory or link.
pub fn validate_result_path(p: &Path) -> Result<(), &'static str> {
    let mut comps = p.components();
    match comps.next() {
        Some(Component::Prefix(pre)) if matches!(pre.kind(), Prefix::Disk(_)) => {}
        _ => return Err("--result must be an absolute drive path"),
    }
    if comps.next() != Some(Component::RootDir) {
        return Err("--result must be an absolute drive path");
    }
    for c in comps {
        match c {
            Component::Normal(_) => {}
            _ => return Err("--result must not contain '.' or '..'"),
        }
    }
    let s = p.to_str().ok_or("--result is not valid Unicode")?;
    if s[2..].contains(':') || s.contains(['*', '?', '"', '<', '>', '|']) {
        return Err("--result contains forbidden characters");
    }
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("--result has no file name")?;
    if !name.to_ascii_lowercase().ends_with(".json") {
        return Err("--result must be a .json file");
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err("--result uses a reserved device name");
    }
    if let Ok(md) = std::fs::symlink_metadata(p) {
        if md.is_dir() {
            return Err("--result is a directory");
        }
        if md.file_type().is_symlink() {
            return Err("--result is a link");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    const RES: &str = r"C:\Temp\r.json";

    #[test]
    fn parses_a_full_request_in_any_order() {
        let r = parse_args(&os(&[
            "--result",
            RES,
            "--arg",
            "5",
            "--fix",
            "end_process",
        ]))
        .unwrap();
        assert_eq!(
            r,
            Request {
                fix: "end_process".into(),
                arg: Some("5".into()),
                result: PathBuf::from(RES)
            }
        );
        let r = parse_args(&os(&["--fix", "clean_temp", "--result", RES])).unwrap();
        assert_eq!(r.arg, None);
    }

    #[test]
    fn arg_values_may_look_like_anything() {
        let r = parse_args(&os(&["--fix", "x", "--arg", "--fix", "--result", RES])).unwrap();
        assert_eq!(r.arg.as_deref(), Some("--fix"));
        assert_eq!(r.fix, "x");
    }

    #[test]
    fn duplicates_unknown_flags_and_missing_values_are_rejected() {
        let bad: &[&[&str]] = &[
            &["--fix", "a", "--fix", "a", "--result", RES],
            &["--fix", "a", "--arg", "1", "--arg", "1", "--result", RES],
            &["--fix", "a", "--evil", "--result", RES],
            &["--fix", "a", "-f", "--result", RES],
            &["--fix", "a", "--result", RES, "stray"],
            &["--fix", "a", "--result", RES, "--arg"],
            &["--fix", "a", "--result"],
            &["--fix", "a"],
            &["--result", RES],
            &[],
        ];
        for args in bad {
            assert!(parse_args(&os(args)).is_err(), "{args:?}");
        }
    }

    #[test]
    fn failures_keep_a_single_valid_result_path() {
        let e = parse_args(&os(&["--fix", "a", "--evil", "--result", RES])).unwrap_err();
        assert_eq!(e.result, Some(PathBuf::from(RES)));
        let e = parse_args(&os(&["--fix", "a", "--result", RES, "--result", RES])).unwrap_err();
        assert_eq!(e.result, None);
        let e = parse_args(&os(&["--fix", "a", "--result", "x.json"])).unwrap_err();
        assert_eq!(e.result, None);
    }

    #[test]
    fn result_path_rules() {
        for good in [r"C:\Temp\r.json", r"D:\Mini Yönetim Uygulaması\sonuç.JSON"] {
            assert_eq!(validate_result_path(Path::new(good)), Ok(()), "{good}");
        }
        for bad in [
            "r.json",
            r"Temp\r.json",
            r"\Temp\r.json",
            r"C:r.json",
            r"\\.\PhysicalDrive0",
            r"\\?\C:\Temp\r.json",
            r"\\server\share\r.json",
            r"C:\Temp\..\Windows\r.json",
            r"C:\Temp\r.json:stream",
            r"C:\Temp\r.txt",
            r"C:\Temp\NUL.json",
            r"C:\Temp\com1.x.json",
            r"C:\Temp\r?.json",
        ] {
            assert!(validate_result_path(Path::new(bad)).is_err(), "{bad}");
        }
        let dir = std::env::temp_dir().join("fix-helper-cli-dir.json");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(validate_result_path(&dir).is_err());
        let _ = std::fs::remove_dir(&dir);
    }
}

//! End-to-end tests of the helper binary: exit codes, result file, log file.
//! No real fix is run: only invalid requests, a startup id that does not
//! exist, and deferred fixes (which execute nothing).

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_fix-helper");

/// A fresh, empty directory for one test.
fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("fix-helper-test-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

struct Run {
    code: i32,
    result: Option<Value>,
}

/// Runs the helper with `args`, `--result <dir>/result.json` appended unless
/// `with_result` is false, and the log redirected into `dir/logs`.
fn run_in(dir: &Path, args: &[&str], with_result: bool) -> Run {
    let result = dir.join("result.json");
    let mut cmd = Command::new(BIN);
    cmd.args(args).env("SAGLIK_LOG_DIR", dir.join("logs"));
    if with_result {
        cmd.arg("--result").arg(&result);
    }
    let out = cmd.output().expect("helper runs");
    let result = std::fs::read_to_string(&result)
        .ok()
        .map(|s| serde_json::from_str(&s).expect("result is JSON"));
    Run {
        code: out.status.code().expect("exit code"),
        result,
    }
}

fn assert_failed_result(r: &Run, key: &str) {
    let v = r.result.as_ref().expect("result file written");
    assert_eq!(v["ok"], Value::Bool(false), "{v}");
    assert_eq!(v["message"], Value::String(key.into()), "{v}");
    assert!(v["log"].is_string(), "{v}");
    assert!(v["params"].is_object(), "{v}");
}

#[test]
fn unknown_id_exits_with_code_2_and_runs_nothing() {
    let dir = scratch("unknown");
    let sentinel = dir.join("pwned.txt");
    let hostile = format!("clean_temp & echo x > \"{}\"", sentinel.display());
    for id in ["format_c", "CleanTemp", "clean_temp ", "", hostile.as_str()] {
        let r = run_in(&dir, &["--fix", id], true);
        assert_eq!(r.code, 2, "id {id:?}");
        assert_failed_result(&r, "fix.result.invalid");
    }
    assert!(!sentinel.exists());
}

#[test]
fn shell_metacharacters_in_arg_are_not_executed() {
    let dir = scratch("shell");
    let sentinel = dir.join("pwned.txt");
    let echo = format!("x & echo x > \"{}\"", sentinel.display());
    for fix in ["end_process", "disable_startup_item"] {
        for arg in ["x & calc", echo.as_str(), "1 | calc", "$(calc)"] {
            let r = run_in(&dir, &["--fix", fix, "--arg", arg], true);
            assert_eq!(r.code, 2, "{fix} {arg:?}");
            assert_failed_result(&r, "fix.result.invalid");
        }
    }
    // Well-formed startup id carrying metacharacters: it is only compared as
    // data against the real Run entries, finds nothing, and runs nothing.
    let id = format!("HKCU|zz-no-such-item & echo x > \"{}\"", sentinel.display());
    let r = run_in(&dir, &["--fix", "disable_startup_item", "--arg", &id], true);
    assert_eq!(r.code, 3);
    assert_failed_result(&r, "fix.result.item_not_found");
    assert!(!sentinel.exists());
}

#[test]
fn result_file_is_written_even_on_failure() {
    let dir = scratch("failure");
    // Invalid argument.
    let r = run_in(&dir, &["--fix", "end_process", "--arg", "abc"], true);
    assert_eq!(r.code, 2);
    assert_failed_result(&r, "fix.result.invalid");
    // Apply error: no such startup item.
    let r = run_in(
        &dir,
        &[
            "--fix",
            "disable_startup_item",
            "--arg",
            "HKCU|zz-no-such-item",
        ],
        true,
    );
    assert_eq!(r.code, 3);
    assert_failed_result(&r, "fix.result.item_not_found");
}

#[test]
fn log_written_to_path_with_unicode_and_spaces() {
    let base = scratch("log").join("Mini Yönetim Uygulaması");
    let logs = base.join("günlük klasörü");
    let result = base.join("sonuç dosyası.json");
    std::fs::create_dir_all(&base).unwrap();
    let out = Command::new(BIN)
        .args([
            "--fix",
            "disable_startup_item",
            "--arg",
            "HKCU|yok
kayıt",
            "--result",
        ])
        .arg(&result)
        .env("SAGLIK_LOG_DIR", &logs)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(result.is_file());
    let text = std::fs::read_to_string(logs.join("fix.log")).expect("log written");
    let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["fix"], "disable_startup_item");
    // The control character is escaped, the Turkish letters are kept.
    assert_eq!(last["arg"], r"HKCU|yok\u{a}kayıt");
    assert_eq!(last["outcome"], "rejected");
    assert_eq!(last["message"], "fix.result.invalid");
    assert!(last["ts"].as_str().unwrap().ends_with('Z'));
}

#[test]
fn malformed_command_lines_are_rejected() {
    let dir = scratch("cli");
    let res = dir.join("result.json");
    let res = res.to_str().unwrap();
    let cases: &[&[&str]] = &[
        &[
            "--fix",
            "clean_temp",
            "--fix",
            "clean_temp",
            "--result",
            res,
        ],
        &["--fix", "clean_temp", "--evil", "x", "--result", res],
        &["--fix", "clean_temp", "--result", res, "extra"],
        &["--fix", "clean_temp", "--result", res, "--arg"],
        &["--fix", "clean_temp", "--result", res, "--result", res],
        &["--fix"],
        &["--fix", "clean_temp"],
        &["--fix", "clean_temp", "--result", "relative.json"],
        &["--fix", "clean_temp", "--result", r"\\.\PhysicalDrive0"],
        &[],
    ];
    for args in cases {
        let r = run_in(&dir, args, false);
        assert_eq!(r.code, 2, "{args:?}");
    }
    // A result path that is an existing directory.
    let as_dir = dir.join("adir.json");
    std::fs::create_dir_all(&as_dir).unwrap();
    let out = Command::new(BIN)
        .args(["--fix", "clean_temp", "--result"])
        .arg(&as_dir)
        .env("SAGLIK_LOG_DIR", dir.join("logs"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn missing_arg_for_disable_startup_item_is_rejected() {
    let dir = scratch("noarg");
    let r = run_in(&dir, &["--fix", "disable_startup_item"], true);
    assert_eq!(r.code, 2);
    assert_failed_result(&r, "fix.result.invalid");
    let r = run_in(&dir, &["--fix", "clean_temp", "--arg", "x"], true);
    assert_eq!(r.code, 2);
}

#[test]
fn deferred_fixes_exit_3_and_run_nothing() {
    let dir = scratch("deferred");
    let cases: &[&[&str]] = &[
        &["--fix", "end_process", "--arg", "1234"],
        &["--fix", "clean_update_leftovers"],
        &["--fix", "reset_windows_update"],
        &["--fix", "enable_defender_realtime"],
        &["--fix", "enable_firewall"],
        &["--fix", "run_sfc_dism"],
    ];
    for args in cases {
        let r = run_in(&dir, args, true);
        assert_eq!(r.code, 3, "{args:?}");
        assert_failed_result(&r, "fix.result.not_available");
    }
}

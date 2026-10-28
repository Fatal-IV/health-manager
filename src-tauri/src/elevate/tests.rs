use super::*;
use std::cell::RefCell;

struct Fake {
    calls: RefCell<Vec<(HelperArgs, bool)>>,
    reply: Result<LaunchStatus, LaunchError>,
    write: Option<String>,
    progress: Option<Vec<u8>>,
    saw_progress_file: std::cell::Cell<bool>,
}
impl Fake {
    fn new(reply: Result<LaunchStatus, LaunchError>, write: Option<&str>) -> Self {
        Fake {
            calls: RefCell::new(vec![]),
            reply,
            write: write.map(String::from),
            progress: None,
            saw_progress_file: std::cell::Cell::new(false),
        }
    }
    fn with_progress(mut self, bytes: &[u8]) -> Self {
        self.progress = Some(bytes.to_vec());
        self
    }
}
impl Launcher for Fake {
    fn run_helper(&self, args: &HelperArgs, elevate: bool) -> Result<LaunchStatus, LaunchError> {
        self.calls.borrow_mut().push((args.clone(), elevate));
        let pp = progress_path(&args.result);
        self.saw_progress_file.set(pp.exists());
        if let Some(b) = &self.progress {
            std::fs::write(&pp, b).unwrap();
        }
        if let Some(w) = &self.write {
            std::fs::write(&args.result, w).unwrap();
        }
        self.reply.clone()
    }
}
fn exited(c: u32) -> Result<LaunchStatus, LaunchError> {
    Ok(LaunchStatus::Exited(c))
}
const OK_JSON: &str = r#"{"ok":true,"message":"fix.result.ok","params":{"n":"3"},"log":"x"}"#;

fn failed_key(o: &FixOutcome) -> &str {
    match o {
        FixOutcome::Failed { message, .. } => message,
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn uac_cancelled_maps_to_declined() {
    let f = Fake::new(Err(LaunchError::Cancelled), None);
    assert_eq!(apply_with(&f, FixId::EnableFirewall, None, None), FixOutcome::Declined);
}
#[test]
fn non_admin_fix_does_not_request_elevation() {
    let f = Fake::new(exited(0), Some(OK_JSON));
    let o = apply_with(&f, FixId::CleanTemp, None, None);
    assert!(matches!(o, FixOutcome::Applied { .. }), "{o:?}");
    assert!(!f.calls.borrow()[0].1);
}
#[test]
fn admin_fix_requests_elevation() {
    let f = Fake::new(
        exited(3),
        Some(r#"{"ok":false,"message":"fix.result.not_available","log":""}"#),
    );
    let o = apply_with(&f, FixId::EnableFirewall, None, None);
    assert!(f.calls.borrow()[0].1);
    assert_eq!(failed_key(&o), "fix.result.not_available");
}
#[test]
fn missing_result_file_maps_to_failed() {
    let f = Fake::new(exited(0), None);
    assert_eq!(failed_key(&apply_with(&f, FixId::CleanTemp, None, None)), "fix.result.failed");
}
#[test]
fn helper_exit_2_maps_to_failed() {
    let f = Fake::new(exited(2), None);
    assert_eq!(
        failed_key(&apply_with(&f, FixId::CleanTemp, None, None)),
        "fix.result.invalid_argument"
    );
}
#[test]
fn invalid_arg_launches_nothing() {
    let f = Fake::new(exited(0), Some(OK_JSON));
    let o = apply_with(&f, FixId::DisableStartupItem, None, None);
    assert_eq!(failed_key(&o), "fix.result.invalid_argument");
    assert!(f.calls.borrow().is_empty());
}
#[test]
fn timeout_maps_to_failed_timeout() {
    let f = Fake::new(Ok(LaunchStatus::TimedOut), None);
    assert_eq!(failed_key(&apply_with(&f, FixId::CleanTemp, None, None)), "fix.result.timeout");
}
#[test]
fn launch_error_maps_to_failed() {
    let f = Fake::new(Err(LaunchError::Other("boom".into())), None);
    let o = apply_with(&f, FixId::CleanTemp, None, None);
    assert_eq!(failed_key(&o), "fix.result.failed");
}
#[test]
fn hostile_message_keys_are_replaced() {
    for bad in [
        "<script>",
        "fix.result.ok; calc",
        "fix.result.",
        "other.key",
        "fix.result.OK",
        "fix.result.a\n",
    ] {
        let json = serde_json::json!({"ok":false,"message":bad,"log":""}).to_string();
        let f = Fake::new(exited(3), Some(&json));
        assert_eq!(failed_key(&apply_with(&f, FixId::CleanTemp, None, None)), "fix.result.failed", "{bad:?}");
    }
    let f = Fake::new(
        exited(3),
        Some(r#"{"ok":false,"message":"fix.result.item_not_found","log":""}"#),
    );
    assert_eq!(
        failed_key(&apply_with(&f, FixId::CleanTemp, None, None)),
        "fix.result.item_not_found"
    );
}
#[test]
fn exit_0_with_ok_false_is_failed() {
    let f = Fake::new(exited(0), Some(r#"{"ok":false,"message":"fix.result.ok","log":""}"#));
    assert!(matches!(
        apply_with(&f, FixId::CleanTemp, None, None),
        FixOutcome::Failed { .. }
    ));
}
#[test]
fn oversized_and_garbled_results_fail() {
    let big = format!(
        r#"{{"ok":true,"message":"fix.result.ok","log":"{}"}}"#,
        "a".repeat(70_000)
    );
    for body in [big, "not json".to_string(), String::new()] {
        let f = Fake::new(exited(0), Some(&body));
        assert_eq!(failed_key(&apply_with(&f, FixId::CleanTemp, None, None)), "fix.result.failed");
        assert!(!f.calls.borrow()[0].0.result.exists());
    }
}
#[test]
fn non_utf8_result_is_rejected() {
    struct Bin;
    impl Launcher for Bin {
        fn run_helper(&self, a: &HelperArgs, _: bool) -> Result<LaunchStatus, LaunchError> {
            std::fs::write(&a.result, [0xff, 0xfe, 0x7b]).unwrap();
            Ok(LaunchStatus::Exited(0))
        }
    }
    assert_eq!(failed_key(&apply_with(&Bin, FixId::CleanTemp, None, None)), "fix.result.failed");
}
#[test]
fn params_are_truncated() {
    let long = "x".repeat(5000);
    let json =
        serde_json::json!({"ok":true,"message":"fix.result.ok","params":{"k":long},"log":""}).to_string();
    let f = Fake::new(exited(0), Some(&json));
    match apply_with(&f, FixId::CleanTemp, None, None) {
        FixOutcome::Applied { params, .. } => assert!(params["k"].chars().count() <= MAX_PARAM_LEN),
        o => panic!("{o:?}"),
    }
}
#[test]
fn result_file_deleted_after_reading() {
    let f = Fake::new(exited(0), Some(OK_JSON));
    match apply_with(&f, FixId::CleanTemp, None, None) {
        FixOutcome::Applied { message, params } => {
            assert_eq!(message, "fix.result.ok");
            assert_eq!(params["n"], "3");
        }
        o => panic!("{o:?}"),
    }
    assert!(!f.calls.borrow()[0].0.result.exists());
}
#[test]
fn result_path_is_absolute_unique_nonexisting() {
    let a = new_result_path().unwrap();
    let b = new_result_path().unwrap();
    assert_ne!(a, b);
    for p in [&a, &b] {
        assert!(p.is_absolute());
        assert!(!p.exists());
        assert_eq!(p.extension().unwrap(), "json");
        let n = p.file_name().unwrap().to_str().unwrap();
        assert!(
            n.starts_with("saglik-denetimi-") && n.len() == "saglik-denetimi-".len() + 32 + 5,
            "{n}"
        );
    }
}
#[test]
fn argv_shape() {
    let a = HelperArgs {
        fix: "disable_startup_item".into(),
        arg: Some("HKCU|x".into()),
        result: PathBuf::from("C:\\t\\r.json"),
    };
    assert_eq!(
        a.argv(),
        ["--fix", "disable_startup_item", "--arg", "HKCU|x", "--result", "C:\\t\\r.json"]
    );
    let b = HelperArgs { arg: None, ..a };
    assert_eq!(b.argv().len(), 4);
}

#[test]
fn quoting_rules() {
    assert_eq!(quote_windows_arg(""), "\"\"");
    assert_eq!(quote_windows_arg("abc"), "abc");
    assert_eq!(quote_windows_arg("a b"), "\"a b\"");
    assert_eq!(quote_windows_arg("x & calc"), "\"x & calc\"");
    assert_eq!(quote_windows_arg("\"; calc"), "\"\\\"; calc\"");
    assert_eq!(quote_windows_arg("C:\\dir\\"), "C:\\dir\\");
    assert_eq!(quote_windows_arg("C:\\my dir\\"), "\"C:\\my dir\\\\\"");
    assert_eq!(quote_windows_arg("a\\\"b"), "\"a\\\\\\\"b\"");
    assert_eq!(
        quote_windows_arg("C:\\Users\\Mini Yönetim Uygulaması\\x.json"),
        "\"C:\\Users\\Mini Yönetim Uygulaması\\x.json\""
    );
}

/// Reference CommandLineToArgvW-style parser (no program name).
fn split(s: &str) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    loop {
        while i < c.len() && (c[i] == ' ' || c[i] == '\t') {
            i += 1;
        }
        if i >= c.len() {
            break;
        }
        let (mut cur, mut inq) = (String::new(), false);
        while i < c.len() {
            let mut n = 0;
            while i < c.len() && c[i] == '\\' {
                n += 1;
                i += 1;
            }
            if i < c.len() && c[i] == '"' {
                cur.extend(std::iter::repeat_n('\\', n / 2));
                if n % 2 == 1 {
                    cur.push('"');
                } else {
                    inq = !inq;
                }
                i += 1;
            } else {
                cur.extend(std::iter::repeat_n('\\', n));
                if i >= c.len() {
                    break;
                }
                if !inq && (c[i] == ' ' || c[i] == '\t') {
                    break;
                }
                cur.push(c[i]);
                i += 1;
            }
        }
        out.push(cur);
    }
    out
}

#[test]
fn build_parameters_round_trips_hostile_values() {
    let vals = [
        "x & calc",
        "\"; calc",
        "a\\",
        "a b\\",
        "",
        "\\\\",
        "\"",
        "\\\"",
        "a\"b c",
        "C:\\Users\\Mini Yönetim Uygulaması\\x.json",
        "tab\there",
        "%PATH% ^ | >",
    ];
    for v in vals {
        let args = HelperArgs {
            fix: "disable_startup_item".into(),
            arg: Some(v.into()),
            result: PathBuf::from("C:\\Users\\Mini Yönetim Uygulaması\\x.json"),
        };
        let params = build_parameters(&args);
        assert_eq!(split(&params), args.argv(), "{v:?} -> {params}");
    }
}

#[test]
fn locate_helper_finds_sibling_only() {
    let dir = std::env::temp_dir().join(format!("elev-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("app.exe");
    assert!(locate_helper(&exe).is_none());
    std::fs::write(dir.join("fix-helper.exe"), b"").unwrap();
    assert_eq!(locate_helper(&exe).unwrap(), dir.join("fix-helper.exe"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_produced_key_exists_in_tr_json() {
    let tr: serde_json::Value =
        serde_json::from_str(include_str!("../../../ui/src/i18n/tr.json")).unwrap();
    for k in PRODUCED_KEYS {
        let leaf = k.strip_prefix("fix.result.").unwrap();
        assert!(tr["fix"]["result"][leaf].is_string(), "missing {k}");
        assert!(is_valid_key(k));
    }
}

#[test]
fn parse_progress_takes_the_highest_valid_op() {
    assert_eq!(parse_progress("{\"op\":1}
{\"op\":3}
", 3, 0), 3);
    assert_eq!(parse_progress("{\"op\":1}
", 3, 0), 1);
}
#[test]
fn parse_progress_ignores_bad_input() {
    for bad in ["", "garbage
", "{\"op\":99}
", "{\"op\":-1}
", "{\"op\":1.5}
", "{\"op\":1", "{\"x\":1}
"] {
        assert_eq!(parse_progress(bad, 3, 1), 1, "{bad:?}");
    }
    // A value that goes backwards never lowers `last`.
    assert_eq!(parse_progress("{\"op\":1}
", 3, 2), 2);
    // Only the first 4 KiB are looked at.
    let late = format!("{}{{\"op\":3}}
", "x".repeat(5000));
    assert_eq!(parse_progress(&late, 3, 0), 0);
}
#[test]
fn read_progress_ignores_non_utf8_and_missing_files() {
    let dir = std::env::temp_dir().join(format!("elevate-progress-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("p.progress");
    std::fs::write(&f, [0xff, 0xfe, b'{', b'"']).unwrap();
    assert_eq!(read_progress(&f, 3, 1), 1);
    assert_eq!(read_progress(&dir.join("missing.progress"), 3, 1), 1);
}
#[test]
fn apply_with_reports_progress_and_still_returns_the_result() {
    let f = Fake::new(exited(0), Some(OK_JSON)).with_progress(b"{\"op\":1}
{\"op\":2}
");
    let seen = std::sync::Mutex::new(Vec::<u32>::new());
    let cb = |d: u32| seen.lock().unwrap().push(d);
    let o = apply_with(&f, FixId::RenewIp, None, Some(&cb));
    assert!(matches!(o, FixOutcome::Applied { .. }), "{o:?}");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.last(), Some(&2));
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
    assert!(f.saw_progress_file.get(), "the app creates the file before the helper starts");
    let path = progress_path(&f.calls.borrow()[0].0.result);
    assert!(!path.exists(), "progress file is removed");
}
#[test]
fn apply_with_ignores_hostile_progress_file() {
    let junk = vec![b'x'; 8192];
    let f = Fake::new(exited(0), Some(OK_JSON)).with_progress(&junk);
    let calls = std::sync::atomic::AtomicU32::new(0);
    let cb = |_: u32| {
        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    };
    let o = apply_with(&f, FixId::RenewIp, None, Some(&cb));
    assert!(matches!(o, FixOutcome::Applied { .. }), "{o:?}");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}
#[test]
fn timeout_still_stops_the_reader_and_removes_the_file() {
    let f = Fake::new(Ok(LaunchStatus::TimedOut), None).with_progress(b"{\"op\":1}
");
    let cb = |_: u32| {};
    assert_eq!(failed_key(&apply_with(&f, FixId::RenewIp, None, Some(&cb))), "fix.result.timeout");
    assert!(!progress_path(&f.calls.borrow()[0].0.result).exists());
}
#[test]
fn no_progress_file_without_a_callback() {
    let f = Fake::new(exited(0), Some(OK_JSON));
    let _ = apply_with(&f, FixId::RenewIp, None, None);
    assert!(!f.saw_progress_file.get());
}

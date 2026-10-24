//! Tauri commands. The logic lives in small functions that take their
//! dependencies as parameters (probe, fix applier, clock) so it can be tested
//! without a window, a real system or a UAC prompt; the `#[tauri::command]`
//! wrappers at the bottom only supply the real ones.

use crate::elevate::FixOutcome;
use health_core::checks::all_checks;
use health_core::engine::{run_scan, Check, Progress, ScanMode};
use health_core::fixes::{fix_plan, validate_arg, FixId, FixPlan};
use health_core::probe::SystemProbe;
use health_core::report::{iso_utc, translate, write_report, AppliedFix, ReportFormat, ScanReport};
use health_core::verify::rescan_verify;
use serde::Serialize;
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// What the UI can ask for after a scan: the last report and the fixes applied.
#[derive(Default, Serialize, Clone, Debug, PartialEq)]
pub struct Session {
    pub report: Option<ScanReport>,
    pub applied: Vec<AppliedFix>,
}

#[derive(Default)]
pub struct AppState {
    pub cancel: Arc<AtomicBool>,
    pub scanning: Arc<AtomicBool>,
    pub session: Arc<Mutex<Session>>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ApplyResult {
    pub outcome: FixOutcome,
    /// True only if a re-scan no longer shows the problem.
    pub verified: bool,
    /// The report after the re-scan (unchanged when nothing was applied).
    pub report: Option<ScanReport>,
}

/// Parses the id and checks the argument; nothing else may run before this.
pub fn parse_fix(fix: &str, arg: Option<&str>) -> Result<FixId, String> {
    let id = FixId::from_str(fix).map_err(|e| e.to_string())?;
    validate_arg(id, arg).map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn plan_for(fix: &str, arg: Option<&str>) -> Result<FixPlan, String> {
    let id = parse_fix(fix, arg)?;
    Ok(fix_plan(id, arg))
}

/// Applies a fix, then (only if the helper reports success) re-scans the checks
/// that offered it and records the result in the session.
pub fn apply_and_verify(
    applier: &dyn Fn(FixId, Option<&str>) -> FixOutcome,
    checks: &[Box<dyn Check>],
    probe: &dyn SystemProbe,
    session: &Mutex<Session>,
    fix: &str,
    arg: Option<&str>,
    now: SystemTime,
) -> Result<ApplyResult, String> {
    let id = parse_fix(fix, arg)?;
    let outcome = applier(id, arg);
    let mut s = session
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    if !matches!(outcome, FixOutcome::Applied { .. }) {
        return Ok(ApplyResult {
            outcome,
            verified: false,
            report: s.report.clone(),
        });
    }
    let verified = match s.report.take() {
        Some(prev) => {
            let (merged, ok) = rescan_verify(checks, probe, &prev, id, arg, now);
            s.report = Some(merged);
            ok
        }
        None => false,
    };
    s.applied.push(AppliedFix {
        fix: id,
        outcome: if verified { "applied" } else { "unverified" }.to_string(),
        at: iso_utc(now),
    });
    Ok(ApplyResult {
        outcome,
        verified,
        report: s.report.clone(),
    })
}

pub fn export(session: &Mutex<Session>, format: ReportFormat, path: &str) -> Result<(), String> {
    let s = session
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    let report = s.report.as_ref().ok_or_else(|| "no_report".to_string())?;
    write_report(format, Path::new(path), report, &s.applied, &translate).map_err(|e| e.to_string())
}

/// Clears the "scanning" flag on every exit path.
struct ScanGuard(Arc<AtomicBool>);
impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Runs a scan and stores the report in the session. Only one scan at a time.
pub fn scan_and_store(
    state: &AppState,
    probe: &dyn SystemProbe,
    mode: ScanMode,
    progress: &dyn Fn(Progress),
    now: SystemTime,
) -> Result<ScanReport, String> {
    if state
        .scanning
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("scan_in_progress".into());
    }
    let _guard = ScanGuard(state.scanning.clone());
    state.cancel.store(false, Ordering::SeqCst);
    let out = run_scan(&all_checks(), probe, mode, progress, &state.cancel);
    let report = ScanReport::new(out.results, out.cancelled, now);
    let mut s = state
        .session
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    s.report = Some(report.clone());
    s.applied.clear();
    Ok(report)
}

#[cfg(windows)]
fn make_probe() -> health_core::probe::windows::WindowsProbe {
    health_core::probe::windows::WindowsProbe::new()
}

#[tauri::command]
pub async fn start_scan(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    mode: ScanMode,
) -> Result<ScanReport, String> {
    use tauri::Emitter;
    let inner = AppState {
        cancel: state.cancel.clone(),
        scanning: state.scanning.clone(),
        session: state.session.clone(),
    };
    // Some probes sleep (CPU sampling) or call COM: never on the UI thread.
    tauri::async_runtime::spawn_blocking(move || {
        let probe = make_probe();
        scan_and_store(
            &inner,
            &probe,
            mode,
            &|p| {
                let _ = app.emit("scan-progress", p);
            },
            SystemTime::now(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cancel_scan(state: tauri::State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn get_fix_plan(fix: String, arg: Option<String>) -> Result<FixPlan, String> {
    plan_for(&fix, arg.as_deref())
}

#[tauri::command]
pub async fn apply_fix_cmd(
    state: tauri::State<'_, AppState>,
    fix: String,
    arg: Option<String>,
) -> Result<ApplyResult, String> {
    let session = state.session.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let probe = make_probe();
        apply_and_verify(
            &|f, a| crate::elevate::apply_fix(f, a),
            &all_checks(),
            &probe,
            &session,
            &fix,
            arg.as_deref(),
            SystemTime::now(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_session(state: tauri::State<'_, AppState>) -> Result<Session, String> {
    state
        .session
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "session lock poisoned".to_string())
}

#[tauri::command]
pub fn export_report(
    state: tauri::State<'_, AppState>,
    format: ReportFormat,
    path: String,
) -> Result<(), String> {
    export(&state.session, format, &path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use health_core::probe::fake::FakeProbe;
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::time::UNIX_EPOCH;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn ok() -> FixOutcome {
        FixOutcome::Applied {
            message: "fix.result.ok".into(),
            params: BTreeMap::new(),
        }
    }

    fn state_with_scan(p: &FakeProbe) -> AppState {
        let st = AppState::default();
        scan_and_store(&st, p, ScanMode::Quick, &|_| {}, UNIX_EPOCH).unwrap();
        st
    }

    #[test]
    fn parse_fix_rejects_unknown_ids_and_bad_args_before_anything_runs() {
        assert!(parse_fix("format_c", None).is_err());
        assert!(parse_fix("clean_temp", Some("x")).is_err());
        assert!(parse_fix("end_process", None).is_err());
        assert_eq!(parse_fix("clean_temp", None), Ok(FixId::CleanTemp));
    }

    #[test]
    fn invalid_request_never_reaches_the_applier() {
        let called = Cell::new(false);
        let applier = |_: FixId, _: Option<&str>| {
            called.set(true);
            ok()
        };
        let st = AppState::default();
        let r = apply_and_verify(
            &applier,
            &all_checks(),
            &FakeProbe::default(),
            &st.session,
            "nope",
            None,
            UNIX_EPOCH,
        );
        assert!(r.is_err());
        assert!(!called.get());
    }

    #[test]
    fn applied_and_gone_is_verified_and_recorded() {
        let before = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let st = state_with_scan(&before);
        let score_before = st
            .session
            .lock()
            .unwrap()
            .report
            .as_ref()
            .unwrap()
            .score
            .value;
        let after = FakeProbe::default();
        let r = apply_and_verify(
            &|_, _| ok(),
            &all_checks(),
            &after,
            &st.session,
            "clean_temp",
            None,
            UNIX_EPOCH + std::time::Duration::from_secs(60),
        )
        .unwrap();
        assert!(r.verified);
        let s = st.session.lock().unwrap();
        assert_eq!(s.applied.len(), 1);
        assert_eq!(s.applied[0].outcome, "applied");
        assert_eq!(s.applied[0].at, "1970-01-01T00:01:00Z");
        assert!(r.report.unwrap().score.value > score_before);
    }

    #[test]
    fn applied_but_still_present_is_recorded_as_unverified() {
        let same = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let st = state_with_scan(&same);
        let r = apply_and_verify(
            &|_, _| ok(),
            &all_checks(),
            &same,
            &st.session,
            "clean_temp",
            None,
            UNIX_EPOCH,
        )
        .unwrap();
        assert!(!r.verified);
        assert_eq!(st.session.lock().unwrap().applied[0].outcome, "unverified");
    }

    #[test]
    fn declined_or_failed_changes_nothing() {
        let p = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let st = state_with_scan(&p);
        let before = st.session.lock().unwrap().clone();
        for outcome in [
            FixOutcome::Declined,
            FixOutcome::Failed {
                message: "fix.result.failed".into(),
                params: BTreeMap::new(),
                detail: String::new(),
            },
        ] {
            let o = outcome.clone();
            let r = apply_and_verify(
                &move |_, _| o.clone(),
                &all_checks(),
                &FakeProbe::default(),
                &st.session,
                "clean_temp",
                None,
                UNIX_EPOCH,
            )
            .unwrap();
            assert_eq!(r.outcome, outcome);
            assert!(!r.verified);
        }
        assert_eq!(*st.session.lock().unwrap(), before);
    }

    #[test]
    fn second_scan_while_one_runs_is_refused_and_flag_is_released() {
        let st = AppState::default();
        st.scanning.store(true, Ordering::SeqCst);
        let r = scan_and_store(
            &st,
            &FakeProbe::default(),
            ScanMode::Quick,
            &|_| {},
            UNIX_EPOCH,
        );
        assert_eq!(r.unwrap_err(), "scan_in_progress");
        st.scanning.store(false, Ordering::SeqCst);
        assert!(scan_and_store(
            &st,
            &FakeProbe::default(),
            ScanMode::Quick,
            &|_| {},
            UNIX_EPOCH
        )
        .is_ok());
        assert!(
            !st.scanning.load(Ordering::SeqCst),
            "guard releases the flag"
        );
    }

    #[test]
    fn cancel_flag_is_reset_for_the_next_scan_and_cancel_marks_report() {
        let st = AppState::default();
        st.cancel.store(true, Ordering::SeqCst);
        // a stale cancel from an earlier scan must not abort a new scan
        let r = scan_and_store(
            &st,
            &FakeProbe::default(),
            ScanMode::Quick,
            &|_| {},
            UNIX_EPOCH,
        )
        .unwrap();
        assert!(!r.cancelled);
        // cancel raised from the progress callback stops the scan early
        let cancel = st.cancel.clone();
        let r = scan_and_store(
            &st,
            &FakeProbe::default(),
            ScanMode::Quick,
            &move |_| cancel.store(true, Ordering::SeqCst),
            UNIX_EPOCH,
        )
        .unwrap();
        assert!(r.cancelled);
        assert!(r.results.len() < all_checks().iter().filter(|c| !c.deep()).count());
    }

    #[test]
    fn new_scan_clears_applied_fixes() {
        let p = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let st = state_with_scan(&p);
        apply_and_verify(
            &|_, _| ok(),
            &all_checks(),
            &p,
            &st.session,
            "clean_temp",
            None,
            UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(st.session.lock().unwrap().applied.len(), 1);
        scan_and_store(&st, &p, ScanMode::Quick, &|_| {}, UNIX_EPOCH).unwrap();
        assert!(st.session.lock().unwrap().applied.is_empty());
    }

    #[test]
    fn export_needs_a_report_and_writes_turkish_text() {
        let st = AppState::default();
        assert_eq!(
            export(&st.session, ReportFormat::Text, "C:\\x\\r.txt").unwrap_err(),
            "no_report"
        );

        let p = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let st = state_with_scan(&p);
        let dir = std::env::temp_dir().join("Mini Yönetim Uygulaması export");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rapor.txt");
        export(&st.session, ReportFormat::Text, path.to_str().unwrap()).unwrap();
        let text = String::from_utf8(std::fs::read(&path).unwrap()[3..].to_vec()).unwrap();
        assert!(text.contains("Bilgisayar Sağlık Raporu"));
        assert!(text.contains("Sağlık puanı"));
        assert!(
            !text.contains("issue.storage"),
            "keys must be translated: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_for_returns_the_preview_for_valid_requests_only() {
        let plan = plan_for("reset_windows_update", None).unwrap();
        assert!(plan.needs_admin);
        assert!(plan_for("end_process", Some("4")).is_err());
    }
}

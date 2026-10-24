use crate::model::{Category, CheckResult, Issue};
use crate::probe::{ProbeError, SystemProbe};
use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};

pub trait Check: Send + Sync {
    fn id(&self) -> &'static str;
    fn category(&self) -> Category;
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError>;
    fn deep(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    Quick,
    Deep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub category: Category,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScanOutcome {
    pub results: Vec<CheckResult>,
    pub cancelled: bool,
}

/// Runs the selected checks in order. A check that returns `Err` or panics
/// yields a `CheckResult` with `error` set and no issues; other checks still run.
/// `cancel` is polled before each check; partial results are kept.
pub fn run_scan(
    checks: &[Box<dyn Check>],
    p: &dyn SystemProbe,
    mode: ScanMode,
    progress: &dyn Fn(Progress),
    cancel: &AtomicBool,
) -> ScanOutcome {
    let selected: Vec<&Box<dyn Check>> = checks
        .iter()
        .filter(|c| mode == ScanMode::Deep || !c.deep())
        .collect();
    let total = selected.len();
    let mut results = Vec::with_capacity(total);
    let mut cancelled = false;

    for (i, check) in selected.into_iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        let (issues, error) = match catch_unwind(AssertUnwindSafe(|| check.scan(p))) {
            Ok(Ok(issues)) => (issues, None),
            Ok(Err(e)) => (vec![], Some(e.to_string())),
            Err(payload) => (vec![], Some(panic_message(payload.as_ref()))),
        };
        results.push(CheckResult {
            check_id: check.id().to_string(),
            category: check.category(),
            issues,
            error,
        });
        progress(Progress {
            done: i + 1,
            total,
            category: check.category(),
        });
    }

    ScanOutcome { results, cancelled }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    let msg = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string());
    format!("check panicked: {msg}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::fake::FakeProbe;
    use std::sync::Mutex;

    enum Behavior {
        Ok,
        Err,
        Panic,
    }
    struct T {
        id: &'static str,
        deep: bool,
        b: Behavior,
    }
    impl Check for T {
        fn id(&self) -> &'static str {
            self.id
        }
        fn category(&self) -> Category {
            Category::Storage
        }
        fn deep(&self) -> bool {
            self.deep
        }
        fn scan(&self, _p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
            match self.b {
                Behavior::Ok => Ok(vec![]),
                Behavior::Err => Err(ProbeError::Unavailable("boom".into())),
                Behavior::Panic => panic!("check panicked"),
            }
        }
    }
    fn t(id: &'static str, deep: bool, b: Behavior) -> Box<dyn Check> {
        Box::new(T { id, deep, b })
    }
    fn run(checks: &[Box<dyn Check>], mode: ScanMode) -> ScanOutcome {
        run_scan(
            checks,
            &FakeProbe::default(),
            mode,
            &|_| {},
            &AtomicBool::new(false),
        )
    }

    #[test]
    fn failing_check_does_not_abort_scan() {
        let checks = vec![
            t("a", false, Behavior::Err),
            t("b", false, Behavior::Ok),
            t("c", false, Behavior::Panic),
            t("d", false, Behavior::Ok),
        ];
        // catch_unwind still prints the panic message; silence it for this test.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let out = run(&checks, ScanMode::Quick);
        std::panic::set_hook(prev);
        assert_eq!(out.results.len(), 4);
        assert!(out.results[0].error.as_ref().unwrap().contains("boom"));
        assert!(out.results[0].issues.is_empty());
        assert!(out.results[1].error.is_none());
        assert!(out.results[2]
            .error
            .as_ref()
            .unwrap()
            .contains("check panicked"));
        assert!(out.results[3].error.is_none());
        assert!(!out.cancelled);
    }

    #[test]
    fn quick_scan_skips_deep_checks() {
        let checks = vec![t("a", false, Behavior::Ok), t("b", true, Behavior::Ok)];
        let out = run(&checks, ScanMode::Quick);
        let ids: Vec<_> = out.results.iter().map(|r| r.check_id.as_str()).collect();
        assert_eq!(ids, ["a"]);
    }

    #[test]
    fn deep_scan_includes_deep_checks() {
        let checks = vec![t("a", false, Behavior::Ok), t("b", true, Behavior::Ok)];
        assert_eq!(run(&checks, ScanMode::Deep).results.len(), 2);
    }

    #[test]
    fn cancel_stops_and_marks_cancelled() {
        let checks = vec![t("a", false, Behavior::Ok), t("b", false, Behavior::Ok)];
        let cancel = AtomicBool::new(false);
        let out = run_scan(
            &checks,
            &FakeProbe::default(),
            ScanMode::Quick,
            &|p| {
                if p.done == 1 {
                    cancel.store(true, Ordering::SeqCst);
                }
            },
            &cancel,
        );
        assert!(out.cancelled);
        assert_eq!(out.results.len(), 1);
    }

    #[test]
    fn progress_reports_each_check() {
        let checks = vec![t("a", false, Behavior::Ok), t("b", true, Behavior::Ok)];
        let seen = Mutex::new(vec![]);
        run_scan(
            &checks,
            &FakeProbe::default(),
            ScanMode::Deep,
            &|p| seen.lock().unwrap().push((p.done, p.total)),
            &AtomicBool::new(false),
        );
        assert_eq!(*seen.lock().unwrap(), vec![(1, 2), (2, 2)]);
    }
}

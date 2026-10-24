//! After a fix is applied, re-scan only the checks that offered it and decide
//! whether the problem is really gone. A fix that "succeeded" but left the
//! issue in place must not be reported as solved.

use crate::engine::{scan_one, Check};
use crate::fixes::FixId;
use crate::model::Issue;
use crate::probe::SystemProbe;
use crate::report::ScanReport;
use std::time::SystemTime;

/// Does `issue` still describe the thing `fix`/`arg` was meant to remove?
pub fn issue_targets(issue: &Issue, fix: FixId, arg: Option<&str>) -> bool {
    if issue.fix != Some(fix) {
        return false;
    }
    let Some(arg) = arg else { return true };
    match fix {
        // The startup issue lists every counted item as a JSON array of {id, name}.
        FixId::DisableStartupItem => issue
            .params
            .get("items")
            .and_then(|j| serde_json::from_str::<Vec<serde_json::Value>>(j).ok())
            .map(|items| {
                items
                    .iter()
                    .any(|i| i.get("id").and_then(|v| v.as_str()) == Some(arg))
            })
            .unwrap_or(true),
        FixId::EndProcess => issue.params.get("target").map(|t| t == arg).unwrap_or(true),
        _ => true,
    }
}

/// Re-runs the quick checks that had an issue with this fix, merges the fresh
/// results into `prev` (score recomputed) and returns whether the issue is gone.
///
/// Deep checks are not re-run here (they take minutes); a fix owned by a deep
/// check therefore stays unverified until the next deep scan.
pub fn rescan_verify(
    checks: &[Box<dyn Check>],
    p: &dyn SystemProbe,
    prev: &ScanReport,
    fix: FixId,
    arg: Option<&str>,
    now: SystemTime,
) -> (ScanReport, bool) {
    let owners: Vec<&str> = prev
        .results
        .iter()
        .filter(|r| r.issues.iter().any(|i| i.fix == Some(fix)))
        .map(|r| r.check_id.as_str())
        .collect();
    if owners.is_empty() {
        return (prev.clone(), false);
    }

    let mut results = prev.results.clone();
    let mut verified = true;
    for id in owners {
        let Some(check) = checks.iter().find(|c| c.id() == id) else {
            verified = false;
            continue;
        };
        if check.deep() {
            verified = false;
            continue;
        }
        let fresh = scan_one(check.as_ref(), p);
        if fresh.error.is_some() || fresh.issues.iter().any(|i| issue_targets(i, fix, arg)) {
            verified = false;
        }
        if let Some(slot) = results.iter_mut().find(|r| r.check_id == id) {
            *slot = fresh;
        }
    }
    let mut merged = ScanReport::new(results, prev.cancelled, now);
    merged.scanned_at = prev.scanned_at.clone();
    (merged, verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::all_checks;
    use crate::engine::{run_scan, ScanMode};
    use crate::probe::fake::FakeProbe;
    use crate::probe::{ProbeError, StartupItem};
    use std::sync::atomic::AtomicBool;
    use std::time::UNIX_EPOCH;

    const GIB: u64 = 1024 * 1024 * 1024;

    fn scan(p: &FakeProbe) -> ScanReport {
        let out = run_scan(
            &all_checks(),
            p,
            ScanMode::Quick,
            &|_| {},
            &AtomicBool::new(false),
        );
        ScanReport::new(out.results, out.cancelled, UNIX_EPOCH)
    }

    fn has_fix(r: &ScanReport, fix: FixId) -> bool {
        r.results
            .iter()
            .flat_map(|x| &x.issues)
            .any(|i| i.fix == Some(fix))
    }

    #[test]
    fn fix_is_verified_when_rescan_no_longer_shows_the_issue() {
        let before = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let prev = scan(&before);
        assert!(has_fix(&prev, FixId::CleanTemp));

        let after = FakeProbe {
            temp_bytes: 0,
            ..Default::default()
        };
        let (merged, verified) = rescan_verify(
            &all_checks(),
            &after,
            &prev,
            FixId::CleanTemp,
            None,
            UNIX_EPOCH,
        );
        assert!(verified);
        assert!(!has_fix(&merged, FixId::CleanTemp));
        assert!(merged.score.value > prev.score.value);
        assert_eq!(merged.scanned_at, prev.scanned_at);
    }

    #[test]
    fn apply_fix_cmd_reports_unverified_when_rescan_still_shows_issue() {
        let same = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let prev = scan(&same);
        let (merged, verified) = rescan_verify(
            &all_checks(),
            &same,
            &prev,
            FixId::CleanTemp,
            None,
            UNIX_EPOCH,
        );
        assert!(!verified);
        assert!(has_fix(&merged, FixId::CleanTemp));
    }

    #[test]
    fn rescan_error_is_not_verified() {
        let before = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let prev = scan(&before);
        let broken = FakeProbe {
            temp_error: Some(ProbeError::Unavailable("x".into())),
            ..Default::default()
        };
        let (merged, verified) = rescan_verify(
            &all_checks(),
            &broken,
            &prev,
            FixId::CleanTemp,
            None,
            UNIX_EPOCH,
        );
        assert!(!verified);
        let temp = merged
            .results
            .iter()
            .find(|r| r.check_id == "storage.temp_files")
            .unwrap();
        assert!(temp.error.is_some());
    }

    #[test]
    fn other_checks_are_left_untouched() {
        let before = FakeProbe {
            temp_bytes: 2 * GIB,
            recycle_bin_bytes: 3 * GIB,
            ..Default::default()
        };
        let prev = scan(&before);
        let after = FakeProbe {
            temp_bytes: 0,
            recycle_bin_bytes: 3 * GIB,
            ..Default::default()
        };
        let (merged, _) = rescan_verify(
            &all_checks(),
            &after,
            &prev,
            FixId::CleanTemp,
            None,
            UNIX_EPOCH,
        );
        assert!(has_fix(&merged, FixId::EmptyRecycleBin));
        assert!(!has_fix(&merged, FixId::CleanTemp));
    }

    #[test]
    fn unknown_fix_without_matching_issue_is_unverified_and_changes_nothing() {
        let p = FakeProbe::default();
        let prev = scan(&p);
        let (merged, verified) = rescan_verify(
            &all_checks(),
            &p,
            &prev,
            FixId::EnableFirewall,
            None,
            UNIX_EPOCH,
        );
        assert!(!verified);
        assert_eq!(merged, prev);
    }

    fn startup(n: usize) -> Vec<StartupItem> {
        (0..n)
            .map(|i| StartupItem {
                id: format!("HKCU|app{i}"),
                name: format!("app{i}"),
                impact: crate::probe::Impact::Medium,
            })
            .collect()
    }

    #[test]
    fn startup_item_fix_is_verified_only_when_that_item_is_gone() {
        let before = FakeProbe {
            startup_items: startup(12),
            ..Default::default()
        };
        let prev = scan(&before);
        assert!(has_fix(&prev, FixId::DisableStartupItem));

        // Item 0 disabled but still 11 enabled items: the issue remains, item 0 is gone.
        let after = FakeProbe {
            startup_items: startup(12)[1..].to_vec(),
            ..Default::default()
        };
        let (_, verified) = rescan_verify(
            &all_checks(),
            &after,
            &prev,
            FixId::DisableStartupItem,
            Some("HKCU|app0"),
            UNIX_EPOCH,
        );
        assert!(verified, "the chosen item is no longer listed");

        // Same list as before: the chosen item is still there.
        let (_, verified) = rescan_verify(
            &all_checks(),
            &before,
            &prev,
            FixId::DisableStartupItem,
            Some("HKCU|app0"),
            UNIX_EPOCH,
        );
        assert!(!verified);
    }

    #[test]
    fn issue_targets_matches_ids_with_separators() {
        let mut issue = crate::model::Issue {
            id: "performance.startup_items".into(),
            category: crate::model::Category::Performance,
            status: crate::model::Status::Warning,
            title_key: String::new(),
            why_key: String::new(),
            evidence: String::new(),
            params: Default::default(),
            fix: Some(FixId::DisableStartupItem),
        };
        let items = serde_json::json!([{"id": "HKLM|Vendor\\Up/dater", "name": "x"}]).to_string();
        issue.params.insert("items".into(), items);
        assert!(issue_targets(
            &issue,
            FixId::DisableStartupItem,
            Some("HKLM|Vendor\\Up/dater")
        ));
        assert!(!issue_targets(
            &issue,
            FixId::DisableStartupItem,
            Some("HKLM|other")
        ));
        assert!(!issue_targets(&issue, FixId::CleanTemp, None));
    }
}

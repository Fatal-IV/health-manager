use crate::engine::Check;
use crate::model::{Category, Issue, Status};
use crate::probe::{ProbeError, SystemProbe};
use std::collections::BTreeMap;

const GIB: u64 = 1024 * 1024 * 1024;
/// Disk usage (percent) at or above which a disk is Critical / Warning.
const DISK_CRITICAL_PCT: u64 = 90;
const DISK_WARNING_PCT: u64 = 80;
/// Temp files: Warning at >= 1 GiB (from the brief).
const TEMP_WARN_BYTES: u64 = GIB;
/// Recycle bin and Windows Update leftovers: Warning at >= 1 GiB.
/// The brief gives no number for these; this is a controller ruling.
const RECYCLE_WARN_BYTES: u64 = GIB;
const UPDATE_LEFTOVERS_WARN_BYTES: u64 = GIB;

fn gb_param(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / GIB as f64)
}

fn gib_text(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GIB as f64)
}

pub struct DiskSpaceCheck;
pub struct TempFilesCheck;
pub struct RecycleBinCheck;
pub struct UpdateLeftoversCheck;

impl Check for DiskSpaceCheck {
    fn id(&self) -> &'static str {
        "storage.disk_space"
    }
    fn category(&self) -> Category {
        Category::Storage
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let mut issues = Vec::new();
        for d in p.disks()? {
            // Skip removable media and disks that report no capacity.
            if d.removable || d.total_bytes == 0 {
                continue;
            }
            let free = d.free_bytes.min(d.total_bytes);
            let used = d.total_bytes - free;
            // u128 math: cannot overflow, and total_bytes > 0 here.
            let pct = (used as u128 * 100 / d.total_bytes as u128) as u64;
            let status = if pct >= DISK_CRITICAL_PCT {
                Status::Critical
            } else if pct >= DISK_WARNING_PCT {
                Status::Warning
            } else {
                continue;
            };
            let letter: String = d
                .mount
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            issues.push(Issue {
                id: format!("storage.disk_full.{}", letter.to_lowercase()),
                category: Category::Storage,
                status,
                title_key: "issue.storage.disk_full.title".into(),
                why_key: "issue.storage.disk_full.why".into(),
                evidence: format!(
                    "{} {}% used, {} free of {}",
                    d.mount,
                    pct,
                    gib_text(free),
                    gib_text(d.total_bytes)
                ),
                params: BTreeMap::from([
                    ("drive".to_string(), d.mount.clone()),
                    ("percent".to_string(), pct.to_string()),
                    ("free_gb".to_string(), gb_param(free)),
                    ("total_gb".to_string(), gb_param(d.total_bytes)),
                ]),
                fix: None,
            });
        }
        Ok(issues)
    }
}

fn size_issue(id: &str, key: &str, bytes: u64, threshold: u64, fix: &str) -> Vec<Issue> {
    if bytes < threshold {
        return Vec::new();
    }
    vec![Issue {
        id: id.into(),
        category: Category::Storage,
        status: Status::Warning,
        title_key: format!("issue.storage.{key}.title"),
        why_key: format!("issue.storage.{key}.why"),
        evidence: gib_text(bytes),
        params: BTreeMap::from([("size_gb".to_string(), gb_param(bytes))]),
        fix: Some(fix.into()),
    }]
}

impl Check for TempFilesCheck {
    fn id(&self) -> &'static str {
        "storage.temp_files"
    }
    fn category(&self) -> Category {
        Category::Storage
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        Ok(size_issue(
            "storage.temp_files",
            "temp_files",
            p.temp_bytes()?,
            TEMP_WARN_BYTES,
            "clean_temp",
        ))
    }
}

impl Check for RecycleBinCheck {
    fn id(&self) -> &'static str {
        "storage.recycle_bin"
    }
    fn category(&self) -> Category {
        Category::Storage
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        Ok(size_issue(
            "storage.recycle_bin",
            "recycle_bin",
            p.recycle_bin_bytes()?,
            RECYCLE_WARN_BYTES,
            "empty_recycle_bin",
        ))
    }
}

impl Check for UpdateLeftoversCheck {
    fn id(&self) -> &'static str {
        "storage.update_leftovers"
    }
    fn category(&self) -> Category {
        Category::Storage
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        Ok(size_issue(
            "storage.update_leftovers",
            "update_leftovers",
            p.update_leftovers_bytes()?,
            UPDATE_LEFTOVERS_WARN_BYTES,
            "clean_update_leftovers",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::probe::{fake::FakeProbe, DiskInfo};

    fn disk(mount: &str, total: u64, free: u64, removable: bool) -> DiskInfo {
        DiskInfo {
            mount: mount.into(),
            total_bytes: total,
            free_bytes: free,
            removable,
        }
    }
    fn probe_with(disks: Vec<DiskInfo>) -> FakeProbe {
        FakeProbe {
            disks,
            ..Default::default()
        }
    }

    #[test]
    fn disk_over_90_percent_is_critical() {
        let p = probe_with(vec![disk("C:", 100 * GIB, 5 * GIB, false)]);
        let issues = DiskSpaceCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Critical);
        assert_eq!(issues[0].category, Category::Storage);
        assert!(issues[0].evidence.contains("C:"));
        assert_eq!(issues[0].params["drive"], "C:");
        assert_eq!(issues[0].params["percent"], "95");
        assert_eq!(issues[0].params["free_gb"], "5.0");
        assert_eq!(issues[0].params["total_gb"], "100.0");
    }

    #[test]
    fn disk_over_80_percent_is_warning() {
        let p = probe_with(vec![disk("C:", 100 * GIB, 15 * GIB, false)]);
        let issues = DiskSpaceCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
    }

    fn status_for_free(free_gib: u64) -> Option<Status> {
        let p = probe_with(vec![disk("C:", 100 * GIB, free_gib * GIB, false)]);
        DiskSpaceCheck.scan(&p).unwrap().first().map(|i| i.status)
    }

    #[test]
    fn disk_thresholds_are_exact_at_boundaries() {
        assert_eq!(status_for_free(20), Some(Status::Warning)); // exactly 80%
        assert_eq!(status_for_free(21), None); // 79%
        assert_eq!(status_for_free(10), Some(Status::Critical)); // exactly 90%
        assert_eq!(status_for_free(11), Some(Status::Warning)); // 89%
    }

    #[test]
    fn size_issues_carry_size_param() {
        let p = FakeProbe {
            temp_bytes: 2 * GIB,
            ..Default::default()
        };
        let issues = TempFilesCheck.scan(&p).unwrap();
        assert_eq!(issues[0].params["size_gb"], "2.0");
    }

    #[test]
    fn disk_below_80_percent_has_no_issue() {
        let p = probe_with(vec![disk("C:", 100 * GIB, 50 * GIB, false)]);
        assert!(DiskSpaceCheck.scan(&p).unwrap().is_empty());
    }

    #[test]
    fn disk_with_zero_total_is_ignored() {
        let p = probe_with(vec![disk("D:", 0, 0, false)]);
        assert!(DiskSpaceCheck.scan(&p).unwrap().is_empty());
    }

    #[test]
    fn removable_disks_are_skipped() {
        let p = probe_with(vec![disk("E:", 100 * GIB, GIB, true)]);
        assert!(DiskSpaceCheck.scan(&p).unwrap().is_empty());
    }

    #[test]
    fn multiple_disks_report_each_separately() {
        let p = probe_with(vec![
            disk("C:", 100 * GIB, 5 * GIB, false),
            disk("D:", 100 * GIB, 15 * GIB, false),
        ]);
        let issues = DiskSpaceCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 2);
        assert_ne!(issues[0].id, issues[1].id);
        assert_eq!(issues[0].status, Status::Critical);
        assert_eq!(issues[1].status, Status::Warning);
    }

    #[test]
    fn disk_error_is_propagated() {
        let p = FakeProbe {
            disks_error: Some(ProbeError::Other("x".into())),
            ..Default::default()
        };
        assert!(DiskSpaceCheck.scan(&p).is_err());
    }

    #[test]
    fn temp_over_1gb_is_warning_with_fix() {
        let p = FakeProbe {
            temp_bytes: GIB,
            ..Default::default()
        };
        let issues = TempFilesCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].fix.as_deref(), Some("clean_temp"));
    }

    #[test]
    fn temp_under_1gb_has_no_issue() {
        let p = FakeProbe {
            temp_bytes: GIB - 1,
            ..Default::default()
        };
        assert!(TempFilesCheck.scan(&p).unwrap().is_empty());
    }

    #[test]
    fn recycle_bin_over_1gb_is_warning_with_fix() {
        let p = FakeProbe {
            recycle_bin_bytes: 2 * GIB,
            ..Default::default()
        };
        let issues = RecycleBinCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].fix.as_deref(), Some("empty_recycle_bin"));
        assert!(RecycleBinCheck
            .scan(&FakeProbe::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn update_leftovers_over_1gb_is_warning_with_fix() {
        let p = FakeProbe {
            update_leftovers_bytes: 3 * GIB,
            ..Default::default()
        };
        let issues = UpdateLeftoversCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].fix.as_deref(), Some("clean_update_leftovers"));
        assert!(UpdateLeftoversCheck
            .scan(&FakeProbe::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn registry_contains_storage_checks() {
        let ids: Vec<_> = crate::checks::all_checks().iter().map(|c| c.id()).collect();
        for id in [
            "storage.disk_space",
            "storage.temp_files",
            "storage.recycle_bin",
            "storage.update_leftovers",
        ] {
            assert!(ids.contains(&id), "missing {id}");
        }
    }
}

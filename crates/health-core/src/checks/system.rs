use crate::engine::Check;
use crate::model::{Category, Issue, Status};
use crate::probe::{IntegrityResult, ProbeError, SystemProbe};
use std::collections::BTreeMap;

/// Boot longer than this many seconds is a Warning (exactly 90 is fine).
const BOOT_WARN_SECS: u32 = 90;
/// At most this many device names are listed in `params["devices"]`.
const MAX_LISTED_DEVICES: usize = 5;

pub struct ProblemDevicesCheck;
pub struct BootTimeCheck;
/// The only deep check: SFC/DISM take minutes.
pub struct IntegrityCheck;

fn issue(
    id: &str,
    key: &str,
    status: Status,
    evidence: String,
    params: BTreeMap<String, String>,
    fix: Option<&str>,
) -> Issue {
    Issue {
        id: id.into(),
        category: Category::System,
        status,
        title_key: format!("issue.system.{key}.title"),
        why_key: format!("issue.system.{key}.why"),
        evidence,
        params,
        fix: fix.map(String::from),
    }
}

impl Check for ProblemDevicesCheck {
    fn id(&self) -> &'static str {
        "system.devices"
    }
    fn category(&self) -> Category {
        Category::System
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let devices = p.problem_devices()?;
        if devices.is_empty() {
            return Ok(Vec::new());
        }
        let names: Vec<&str> = devices
            .iter()
            .take(MAX_LISTED_DEVICES)
            .map(|d| d.name.as_str())
            .collect();
        let evidence = devices
            .iter()
            .take(MAX_LISTED_DEVICES)
            .map(|d| format!("{} (code {})", d.name, d.code))
            .collect::<Vec<_>>()
            .join("; ");
        // No automatic fix: drivers come from Windows Update / the manufacturer.
        Ok(vec![issue(
            "system.devices",
            "devices",
            Status::Warning,
            evidence,
            BTreeMap::from([
                ("count".to_string(), devices.len().to_string()),
                ("devices".to_string(), names.join(", ")),
            ]),
            None,
        )])
    }
}

impl Check for BootTimeCheck {
    fn id(&self) -> &'static str {
        "system.boot"
    }
    fn category(&self) -> Category {
        Category::System
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let Some(secs) = p.boot_seconds()? else {
            // No boot event is not "healthy": say it is unknown.
            return Ok(vec![issue(
                "system.boot_unknown",
                "boot_unknown",
                Status::Unknown,
                "no boot performance event found".into(),
                BTreeMap::new(),
                None,
            )]);
        };
        if secs <= BOOT_WARN_SECS {
            return Ok(Vec::new());
        }
        Ok(vec![issue(
            "system.boot",
            "boot",
            Status::Warning,
            format!("last boot took {secs} s"),
            BTreeMap::from([("seconds".to_string(), secs.to_string())]),
            None,
        )])
    }
}

impl Check for IntegrityCheck {
    fn id(&self) -> &'static str {
        "system.integrity"
    }
    fn category(&self) -> Category {
        Category::System
    }
    fn deep(&self) -> bool {
        true
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        // Err (e.g. not elevated) becomes an error card, never a healthy result.
        match p.integrity_scan()? {
            IntegrityResult::Clean => Ok(Vec::new()),
            IntegrityResult::CorruptionFound => Ok(vec![issue(
                "system.integrity",
                "integrity",
                Status::Critical,
                "SFC/DISM reported corrupted system files".into(),
                BTreeMap::new(),
                Some("run_sfc_dism"),
            )]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{run_scan, ScanMode};
    use crate::probe::fake::FakeProbe;
    use crate::probe::DeviceProblem;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn dev(name: &str, code: u32) -> DeviceProblem {
        DeviceProblem {
            name: name.into(),
            code,
        }
    }
    fn boot(s: Option<u32>) -> FakeProbe {
        FakeProbe {
            boot_seconds: s,
            ..Default::default()
        }
    }

    #[test]
    fn problem_device_is_warning_with_no_auto_fix() {
        let p = FakeProbe {
            problem_devices: vec![dev("Wi-Fi Adapter", 28)],
            ..Default::default()
        };
        let issues = ProblemDevicesCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert!(issues[0].fix.is_none());
        assert_eq!(issues[0].params["count"], "1");
        assert_eq!(issues[0].params["devices"], "Wi-Fi Adapter");
        assert_eq!(issues[0].title_key, "issue.system.devices.title");
    }

    #[test]
    fn no_problem_devices_has_no_issue() {
        assert!(ProblemDevicesCheck
            .scan(&FakeProbe::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn device_list_is_capped_to_five_names_but_counts_all() {
        let p = FakeProbe {
            problem_devices: (0..7).map(|i| dev(&format!("d{i}"), 10)).collect(),
            ..Default::default()
        };
        let i = &ProblemDevicesCheck.scan(&p).unwrap()[0];
        assert_eq!(i.params["count"], "7");
        assert_eq!(i.params["devices"], "d0, d1, d2, d3, d4");
    }

    #[test]
    fn problem_devices_error_is_propagated() {
        let p = FakeProbe {
            problem_devices_error: Some(ProbeError::Unavailable("x".into())),
            ..Default::default()
        };
        assert!(ProblemDevicesCheck.scan(&p).is_err());
    }

    #[test]
    fn boot_over_90s_is_warning() {
        let issues = BootTimeCheck.scan(&boot(Some(91))).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].params["seconds"], "91");
        assert!(issues[0].fix.is_none());
    }

    #[test]
    fn boot_boundary_90_is_fine() {
        assert!(BootTimeCheck.scan(&boot(Some(90))).unwrap().is_empty());
        assert!(BootTimeCheck.scan(&boot(Some(12))).unwrap().is_empty());
    }

    #[test]
    fn missing_boot_event_is_unknown() {
        let issues = BootTimeCheck.scan(&boot(None)).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Unknown);
        assert_eq!(issues[0].id, "system.boot_unknown");
    }

    #[test]
    fn boot_error_is_propagated() {
        let p = FakeProbe {
            boot_error: Some(ProbeError::Other("x".into())),
            ..Default::default()
        };
        assert!(BootTimeCheck.scan(&p).is_err());
    }

    #[test]
    fn integrity_check_is_deep_only() {
        assert!(IntegrityCheck.deep());
        assert!(!ProblemDevicesCheck.deep());
        assert!(!BootTimeCheck.deep());
        let deep: Vec<_> = crate::checks::all_checks()
            .iter()
            .filter(|c| c.deep())
            .map(|c| c.id())
            .collect();
        assert_eq!(deep, ["system.integrity"]);
    }

    #[test]
    fn corrupt_files_found_is_critical_with_fix() {
        let p = FakeProbe {
            integrity: Some(IntegrityResult::CorruptionFound),
            ..Default::default()
        };
        let issues = IntegrityCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Critical);
        assert_eq!(issues[0].fix.as_deref(), Some("run_sfc_dism"));
    }

    #[test]
    fn clean_integrity_has_no_issue() {
        assert!(IntegrityCheck
            .scan(&FakeProbe::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn integrity_error_is_never_healthy() {
        let p = FakeProbe {
            integrity_error: Some(ProbeError::Unavailable("needs administrator".into())),
            ..Default::default()
        };
        assert!(IntegrityCheck.scan(&p).is_err());
    }

    #[test]
    fn quick_scan_never_runs_integrity_but_deep_does() {
        let p = FakeProbe::default();
        let checks = crate::checks::all_checks();
        let cancel = AtomicBool::new(false);
        run_scan(&checks, &p, ScanMode::Quick, &|_| {}, &cancel);
        assert_eq!(p.integrity_calls.load(Ordering::SeqCst), 0);
        run_scan(&checks, &p, ScanMode::Deep, &|_| {}, &cancel);
        assert_eq!(p.integrity_calls.load(Ordering::SeqCst), 1);
    }
}

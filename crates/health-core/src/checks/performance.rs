use crate::engine::Check;
use crate::model::{Category, Issue, Status};
use crate::probe::{Impact, ProbeError, SystemProbe};
use std::collections::BTreeMap;

const GIB: u64 = 1024 * 1024 * 1024;
/// RAM usage (percent) at or above which the status is Critical / Warning.
const RAM_CRITICAL_PCT: u64 = 90;
const RAM_WARNING_PCT: u64 = 80;
/// CPU usage (percent) at or above which we warn. Never Critical: one
/// measurement window cannot tell a brief spike from a sustained load.
/// The 85% figure is a controller ruling; the brief gives no number.
const CPU_WARNING_PCT: f32 = 85.0;
/// How many of the heaviest processes are inspected.
const TOP_N: usize = 5;
/// A process is "heavy" at >= 50% CPU or >= 2 GiB RAM (controller ruling).
const PROC_CPU_PCT: f32 = 50.0;
const PROC_RAM_BYTES: u64 = 2 * GIB;
/// More than this many enabled High/Medium startup items triggers a Warning.
const STARTUP_MAX: usize = 10;

fn gb_param(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / GIB as f64)
}

/// RAM evaluation; `total == 0` is an error (no usable measurement), never a divide.
fn ram_issue(p: &dyn SystemProbe) -> Result<Option<Issue>, ProbeError> {
    let (used, total) = p.ram_usage()?;
    if total == 0 {
        return Err(ProbeError::Unavailable("total RAM reported as 0".into()));
    }
    let used = used.min(total);
    let pct = (used as u128 * 100 / total as u128) as u64;
    let status = if pct >= RAM_CRITICAL_PCT {
        Status::Critical
    } else if pct >= RAM_WARNING_PCT {
        Status::Warning
    } else {
        return Ok(None);
    };
    Ok(Some(Issue {
        id: "performance.ram_high".into(),
        category: Category::Performance,
        status,
        title_key: "issue.performance.ram_high.title".into(),
        why_key: "issue.performance.ram_high.why".into(),
        evidence: format!(
            "RAM {pct}% used ({} of {} GB)",
            gb_param(used),
            gb_param(total)
        ),
        params: BTreeMap::from([
            ("percent".to_string(), pct.to_string()),
            ("used_gb".to_string(), gb_param(used)),
            ("total_gb".to_string(), gb_param(total)),
        ]),
        fix: None,
    }))
}

fn unknown_issue(what: &str, err: &ProbeError) -> Issue {
    Issue {
        id: format!("performance.{what}_unknown"),
        category: Category::Performance,
        status: Status::Unknown,
        title_key: format!("issue.performance.{what}_unknown.title"),
        why_key: format!("issue.performance.{what}_unknown.why"),
        evidence: err.to_string(),
        params: BTreeMap::new(),
        fix: None,
    }
}

fn cpu_issue(p: &dyn SystemProbe) -> Result<Option<Issue>, ProbeError> {
    let cpu = p.cpu_percent()?;
    if cpu < CPU_WARNING_PCT {
        return Ok(None);
    }
    let cpu_pct = cpu.round().min(100.0) as u32;
    Ok(Some(Issue {
        id: "performance.cpu_high".into(),
        category: Category::Performance,
        status: Status::Warning,
        title_key: "issue.performance.cpu_high.title".into(),
        why_key: "issue.performance.cpu_high.why".into(),
        evidence: format!("CPU {cpu_pct}%"),
        params: BTreeMap::from([("percent".to_string(), cpu_pct.to_string())]),
        fix: None,
    }))
}

pub struct CpuRamCheck;
pub struct TopProcessesCheck;
pub struct StartupItemsCheck;

impl Check for CpuRamCheck {
    fn id(&self) -> &'static str {
        "performance.cpu_ram"
    }
    fn category(&self) -> Category {
        Category::Performance
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let ram = ram_issue(p);
        let cpu = cpu_issue(p);
        let mut issues = Vec::new();
        match (ram, cpu) {
            (Err(e), Err(_)) => return Err(e),
            (r, c) => {
                // A failed sub-check becomes an Unknown issue, never a silent pass.
                issues.extend(r.unwrap_or_else(|e| Some(unknown_issue("ram", &e))));
                issues.extend(c.unwrap_or_else(|e| Some(unknown_issue("cpu", &e))));
            }
        }
        Ok(issues)
    }
}

impl Check for TopProcessesCheck {
    fn id(&self) -> &'static str {
        "performance.top_processes"
    }
    fn category(&self) -> Category {
        Category::Performance
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let mut issues = Vec::new();
        for proc in p.top_processes(TOP_N)? {
            if proc.cpu < PROC_CPU_PCT && proc.ram_bytes < PROC_RAM_BYTES {
                continue;
            }
            let cpu_pct = proc.cpu.round().max(0.0) as u32;
            issues.push(Issue {
                id: format!("performance.heavy_process.{}", proc.pid),
                category: Category::Performance,
                status: Status::Warning,
                title_key: "issue.performance.heavy_process.title".into(),
                why_key: "issue.performance.heavy_process.why".into(),
                evidence: format!(
                    "{} (pid {}) CPU {}%, RAM {} GB",
                    proc.name,
                    proc.pid,
                    cpu_pct,
                    gb_param(proc.ram_bytes)
                ),
                params: BTreeMap::from([
                    ("process".to_string(), proc.name.clone()),
                    ("percent".to_string(), cpu_pct.to_string()),
                    ("ram_gb".to_string(), gb_param(proc.ram_bytes)),
                    ("target".to_string(), proc.pid.to_string()),
                ]),
                fix: Some("end_process".into()),
            });
        }
        Ok(issues)
    }
}

impl Check for StartupItemsCheck {
    fn id(&self) -> &'static str {
        "performance.startup_items"
    }
    fn category(&self) -> Category {
        Category::Performance
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let items = p.startup_items()?;
        // Low-impact items are not counted; the UI lists the counted ones.
        let counted: Vec<_> = items.iter().filter(|i| i.impact != Impact::Low).collect();
        if counted.len() <= STARTUP_MAX {
            return Ok(Vec::new());
        }
        let list: Vec<_> = counted
            .iter()
            .map(|i| serde_json::json!({"id": i.id, "name": i.name}))
            .collect();
        Ok(vec![Issue {
            id: "performance.startup_items".into(),
            category: Category::Performance,
            status: Status::Warning,
            title_key: "issue.performance.startup_items.title".into(),
            why_key: "issue.performance.startup_items.why".into(),
            evidence: format!("{} startup items", counted.len()),
            params: BTreeMap::from([
                ("count".to_string(), counted.len().to_string()),
                (
                    "items".to_string(),
                    serde_json::Value::Array(list).to_string(),
                ),
            ]),
            fix: Some("disable_startup_item".into()),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;
    use crate::probe::{fake::FakeProbe, Impact, ProcInfo, StartupItem};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn ram(used_pct: u64) -> FakeProbe {
        FakeProbe {
            ram_used: used_pct * GIB,
            ram_total: 100 * GIB,
            ..Default::default()
        }
    }
    fn startup(n: usize, impact: Impact) -> Vec<StartupItem> {
        (0..n)
            .map(|i| StartupItem {
                id: format!("HKCU:app{i}"),
                name: format!("app{i}"),
                impact,
            })
            .collect()
    }

    #[test]
    fn ram_over_90_percent_is_critical() {
        let issues = CpuRamCheck.scan(&ram(92)).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Critical);
        assert_eq!(issues[0].category, Category::Performance);
        assert_eq!(issues[0].params["percent"], "92");
        assert_eq!(issues[0].params["total_gb"], "100.0");
    }

    #[test]
    fn ram_80_percent_is_warning_and_below_is_fine() {
        let w = CpuRamCheck.scan(&ram(80)).unwrap();
        assert_eq!(w[0].status, Status::Warning);
        assert!(CpuRamCheck.scan(&ram(79)).unwrap().is_empty());
        assert_eq!(
            CpuRamCheck.scan(&ram(90)).unwrap()[0].status,
            Status::Critical
        );
    }

    #[test]
    fn cpu_spike_single_sample_is_not_critical() {
        let p = FakeProbe {
            cpu_percent: 99.0,
            ram_total: 10 * GIB,
            ..Default::default()
        };
        let issues = CpuRamCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].params["percent"], "99");
    }

    #[test]
    fn zero_total_ram_is_unknown_not_panic() {
        // total == 0: RAM is unreadable -> Unknown issue, no panic.
        let issues = CpuRamCheck.scan(&FakeProbe::default()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].id, "performance.ram_unknown");
        assert_eq!(issues[0].status, Status::Unknown);
        let p = FakeProbe {
            cpu_error: Some(ProbeError::Other("x".into())),
            ..Default::default()
        };
        assert!(matches!(
            CpuRamCheck.scan(&p),
            Err(ProbeError::Unavailable(_))
        ));
    }

    #[test]
    fn more_than_10_high_impact_startup_items_is_warning() {
        let p = FakeProbe {
            startup_items: startup(11, Impact::High),
            ..Default::default()
        };
        let issues = StartupItemsCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].fix.as_deref(), Some("disable_startup_item"));
        assert_eq!(issues[0].params["count"], "11");
        let items: serde_json::Value = serde_json::from_str(&issues[0].params["items"]).unwrap();
        assert_eq!(items.as_array().unwrap().len(), 11);
        assert_eq!(items[0]["id"], "HKCU:app0");
        assert_eq!(items[0]["name"], "app0");
        let ten = FakeProbe {
            startup_items: startup(10, Impact::High),
            ..Default::default()
        };
        assert!(StartupItemsCheck.scan(&ten).unwrap().is_empty());
    }

    #[test]
    fn more_than_10_medium_startup_items_is_warning_and_low_is_ignored() {
        let medium = FakeProbe {
            startup_items: startup(11, Impact::Medium),
            ..Default::default()
        };
        assert_eq!(StartupItemsCheck.scan(&medium).unwrap().len(), 1);
        let low = FakeProbe {
            startup_items: startup(30, Impact::Low),
            ..Default::default()
        };
        assert!(StartupItemsCheck.scan(&low).unwrap().is_empty());
    }

    #[test]
    fn top_process_issue_names_process_in_evidence() {
        let p = FakeProbe {
            processes: vec![
                ProcInfo {
                    pid: 4242,
                    name: "hog.exe".into(),
                    cpu: 80.0,
                    ram_bytes: GIB,
                },
                ProcInfo {
                    pid: 1,
                    name: "calm.exe".into(),
                    cpu: 1.0,
                    ram_bytes: 10_000_000,
                },
            ],
            ..Default::default()
        };
        let issues = TopProcessesCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].evidence.contains("hog.exe"));
        assert_eq!(issues[0].params["process"], "hog.exe");
        assert_eq!(issues[0].params["target"], "4242");
        assert_eq!(issues[0].fix.as_deref(), Some("end_process"));
    }

    #[test]
    fn probe_errors_propagate_for_startup_and_processes() {
        let p = FakeProbe {
            startup_error: Some(ProbeError::Other("x".into())),
            ..Default::default()
        };
        assert!(StartupItemsCheck.scan(&p).is_err());
        assert!(TopProcessesCheck
            .scan(&FakeProbe {
                processes_error: Some(ProbeError::Other("x".into())),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn ram_error_does_not_hide_cpu_issue() {
        let p = FakeProbe {
            ram_error: Some(ProbeError::Other("x".into())),
            cpu_percent: 95.0,
            ..Default::default()
        };
        let issues = CpuRamCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|i| i.id == "performance.cpu_high"));
        let u = issues
            .iter()
            .find(|i| i.id == "performance.ram_unknown")
            .unwrap();
        assert_eq!(u.status, Status::Unknown);
        assert!(u.evidence.contains('x'));
    }

    #[test]
    fn cpu_error_does_not_hide_ram_issue_and_both_errors_propagate() {
        let mut p = ram(95);
        p.cpu_error = Some(ProbeError::Other("x".into()));
        let issues = CpuRamCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 2);
        assert!(issues
            .iter()
            .any(|i| i.id == "performance.ram_high" && i.status == Status::Critical));
        assert!(issues
            .iter()
            .any(|i| i.id == "performance.cpu_unknown" && i.status == Status::Unknown));
        p.ram_error = Some(ProbeError::Other("y".into()));
        assert!(CpuRamCheck.scan(&p).is_err());
    }

    #[test]
    fn unknown_issues_have_no_fix() {
        let p = FakeProbe {
            cpu_error: Some(ProbeError::Other("x".into())),
            ..Default::default()
        };
        let issues = CpuRamCheck.scan(&p);
        // both fail here -> Err; use single failures instead
        assert!(issues.is_err());
        for p in [
            FakeProbe {
                ram_error: Some(ProbeError::Other("x".into())),
                ..Default::default()
            },
            FakeProbe {
                cpu_error: Some(ProbeError::Other("x".into())),
                ram_total: GIB,
                ..Default::default()
            },
        ] {
            let issues = CpuRamCheck.scan(&p).unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].status, Status::Unknown);
            assert!(issues[0].fix.is_none());
        }
    }

    #[test]
    fn used_above_total_clamps_to_100_percent() {
        let p = FakeProbe {
            ram_used: 20 * GIB,
            ram_total: 10 * GIB,
            ..Default::default()
        };
        let issues = CpuRamCheck.scan(&p).unwrap();
        assert_eq!(issues[0].params["percent"], "100");
        assert_eq!(issues[0].params["used_gb"], "10.0");
    }

    #[test]
    fn startup_fix_id_is_disable_startup_item() {
        let p = FakeProbe {
            startup_items: startup(12, Impact::Medium),
            ..Default::default()
        };
        let issues = StartupItemsCheck.scan(&p).unwrap();
        assert_eq!(issues[0].fix.as_deref(), Some("disable_startup_item"));
    }

    #[test]
    fn registry_contains_performance_checks() {
        let ids: Vec<_> = crate::checks::all_checks().iter().map(|c| c.id()).collect();
        for id in [
            "performance.cpu_ram",
            "performance.top_processes",
            "performance.startup_items",
        ] {
            assert!(ids.contains(&id), "missing {id}");
        }
    }
}

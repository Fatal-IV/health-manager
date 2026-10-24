use crate::engine::Check;
use crate::fixes::FixId;
use crate::model::{Category, Issue, Status};
use crate::probe::{ProbeError, ServiceState, SystemProbe};
use std::collections::BTreeMap;
use std::time::Duration;

const DAY: Duration = Duration::from_secs(86_400);
/// No successful update for this many days: Warning / Critical (inclusive).
const STALE_WARN_DAYS: u64 = 60;
const STALE_CRITICAL_DAYS: u64 = 180;
const UPDATE_SERVICE: &str = "wuauserv";

pub struct PendingUpdatesCheck;
pub struct UpdateServiceCheck;
pub struct LastUpdateCheck;

fn issue(
    id: &str,
    key: &str,
    status: Status,
    evidence: String,
    params: BTreeMap<String, String>,
    fix: Option<FixId>,
) -> Issue {
    Issue {
        id: id.into(),
        category: Category::Update,
        status,
        title_key: format!("issue.update.{key}.title"),
        why_key: format!("issue.update.{key}.why"),
        evidence,
        params,
        fix,
    }
}

impl Check for PendingUpdatesCheck {
    fn id(&self) -> &'static str {
        "update.pending"
    }
    fn category(&self) -> Category {
        Category::Update
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let count = p.pending_updates()?;
        if count == 0 {
            return Ok(Vec::new());
        }
        Ok(vec![issue(
            "update.pending",
            "pending",
            Status::Warning,
            format!("{count} pending update(s)"),
            BTreeMap::from([("count".to_string(), count.to_string())]),
            None,
        )])
    }
}

impl Check for UpdateServiceCheck {
    fn id(&self) -> &'static str {
        "update.service"
    }
    fn category(&self) -> Category {
        Category::Update
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let state = p.service_state(UPDATE_SERVICE)?;
        let evidence = match state {
            // A stopped service is normal: Windows Update starts on demand.
            ServiceState::Running | ServiceState::Stopped => return Ok(Vec::new()),
            ServiceState::Disabled => format!("{UPDATE_SERVICE} is disabled"),
            ServiceState::Missing => format!("{UPDATE_SERVICE} not found"),
        };
        Ok(vec![issue(
            "update.service",
            "service",
            Status::Critical,
            evidence,
            BTreeMap::from([("service".to_string(), UPDATE_SERVICE.to_string())]),
            Some(FixId::ResetWindowsUpdate),
        )])
    }
}

impl Check for LastUpdateCheck {
    fn id(&self) -> &'static str {
        "update.last_success"
    }
    fn category(&self) -> Category {
        Category::Update
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let Some(last) = p.last_update_success()? else {
            return Ok(vec![issue(
                "update.never_updated",
                "never_updated",
                Status::Warning,
                "no successful update recorded".into(),
                BTreeMap::new(),
                None,
            )]);
        };
        // A timestamp in the future (clock skew) counts as 0 days.
        let days = p
            .now()
            .duration_since(last)
            .map(|d| d.as_secs() / DAY.as_secs())
            .unwrap_or(0);
        let status = if days >= STALE_CRITICAL_DAYS {
            Status::Critical
        } else if days >= STALE_WARN_DAYS {
            Status::Warning
        } else {
            return Ok(Vec::new());
        };
        Ok(vec![issue(
            "update.last_success",
            "last_success",
            status,
            format!("last successful update {days} day(s) ago"),
            BTreeMap::from([("days".to_string(), days.to_string())]),
            None,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::fake::FakeProbe;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now() -> SystemTime {
        UNIX_EPOCH + DAY * 1_000
    }
    fn probe_ago(days: u64) -> FakeProbe {
        FakeProbe {
            now: Some(now()),
            last_update: Some(now() - DAY * days as u32),
            ..Default::default()
        }
    }
    fn svc(state: ServiceState) -> FakeProbe {
        FakeProbe {
            services: BTreeMap::from([(UPDATE_SERVICE.to_string(), state)]),
            ..Default::default()
        }
    }

    #[test]
    fn disabled_update_service_is_critical_with_reset_fix() {
        let issues = UpdateServiceCheck
            .scan(&svc(ServiceState::Disabled))
            .unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Critical);
        assert_eq!(issues[0].fix, Some(FixId::ResetWindowsUpdate));
        assert_eq!(issues[0].params["service"], UPDATE_SERVICE);
    }

    #[test]
    fn missing_update_service_is_critical_with_reset_fix() {
        let issues = UpdateServiceCheck
            .scan(&svc(ServiceState::Missing))
            .unwrap();
        assert_eq!(issues[0].status, Status::Critical);
        assert_eq!(issues[0].fix, Some(FixId::ResetWindowsUpdate));
    }

    #[test]
    fn running_or_stopped_update_service_is_fine() {
        for s in [ServiceState::Running, ServiceState::Stopped] {
            assert!(UpdateServiceCheck.scan(&svc(s)).unwrap().is_empty());
        }
    }

    #[test]
    fn service_error_is_propagated() {
        let p = FakeProbe {
            services_error: Some(ProbeError::Unavailable("x".into())),
            ..Default::default()
        };
        assert!(UpdateServiceCheck.scan(&p).is_err());
    }

    #[test]
    fn no_update_in_60_days_is_warning() {
        let issues = LastUpdateCheck.scan(&probe_ago(60)).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].params["days"], "60");
        assert!(issues[0].fix.is_none());
    }

    #[test]
    fn no_update_in_180_days_is_critical() {
        let issues = LastUpdateCheck.scan(&probe_ago(180)).unwrap();
        assert_eq!(issues[0].status, Status::Critical);
        assert_eq!(issues[0].params["days"], "180");
    }

    #[test]
    fn last_update_boundaries() {
        assert!(LastUpdateCheck.scan(&probe_ago(0)).unwrap().is_empty());
        assert!(LastUpdateCheck.scan(&probe_ago(59)).unwrap().is_empty());
        let s = |d| LastUpdateCheck.scan(&probe_ago(d)).unwrap()[0].status;
        assert_eq!(s(179), Status::Warning);
        assert_eq!(s(181), Status::Critical);
    }

    #[test]
    fn update_in_the_future_is_not_an_issue() {
        let p = FakeProbe {
            now: Some(now()),
            last_update: Some(now() + DAY),
            ..Default::default()
        };
        assert!(LastUpdateCheck.scan(&p).unwrap().is_empty());
    }

    #[test]
    fn never_updated_is_warning_not_crash() {
        let p = FakeProbe {
            now: Some(now()),
            last_update: None,
            ..Default::default()
        };
        let issues = LastUpdateCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].title_key, "issue.update.never_updated.title");
    }

    #[test]
    fn last_update_error_is_propagated() {
        let p = FakeProbe {
            last_update_error: Some(ProbeError::Other("x".into())),
            ..Default::default()
        };
        assert!(LastUpdateCheck.scan(&p).is_err());
    }

    #[test]
    fn pending_updates_over_0_is_warning() {
        let p = FakeProbe {
            pending_updates: 3,
            ..Default::default()
        };
        let issues = PendingUpdatesCheck.scan(&p).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, Status::Warning);
        assert_eq!(issues[0].params["count"], "3");
        assert!(issues[0].fix.is_none());
    }

    #[test]
    fn no_pending_updates_has_no_issue() {
        assert!(PendingUpdatesCheck
            .scan(&FakeProbe::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn pending_error_is_propagated() {
        let p = FakeProbe {
            pending_updates_error: Some(ProbeError::Unavailable("x".into())),
            ..Default::default()
        };
        assert!(PendingUpdatesCheck.scan(&p).is_err());
    }

    #[test]
    fn registry_contains_update_checks() {
        let ids: Vec<_> = crate::checks::all_checks().iter().map(|c| c.id()).collect();
        for id in ["update.pending", "update.service", "update.last_success"] {
            assert!(ids.contains(&id), "missing {id}");
        }
    }
}

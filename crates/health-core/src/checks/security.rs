use crate::engine::Check;
use crate::model::{Category, Issue, Status};
use crate::probe::{ProbeError, SystemProbe, UacLevel};
use std::collections::BTreeMap;

/// Signatures older than this many days (strictly more) are a Warning.
const SIGNATURE_WARN_DAYS: u32 = 7;

pub struct DefenderCheck;
pub struct FirewallCheck;
pub struct UacCheck;

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
        category: Category::Security,
        status,
        title_key: format!("issue.security.{key}.title"),
        why_key: format!("issue.security.{key}.why"),
        evidence,
        params,
        fix: fix.map(String::from),
    }
}

impl Check for DefenderCheck {
    fn id(&self) -> &'static str {
        "security.defender"
    }
    fn category(&self) -> Category {
        Category::Security
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let d = p.defender()?;
        if !d.realtime {
            // Another antivirus is in charge: Defender being off is expected, but we
            // cannot vouch for that product, so the result is Unknown (not Critical).
            if d.third_party_av {
                return Ok(vec![issue(
                    "security.defender_third_party",
                    "defender_third_party",
                    Status::Unknown,
                    "real-time protection off; another antivirus is registered".into(),
                    BTreeMap::new(),
                    None,
                )]);
            }
            return Ok(vec![issue(
                "security.defender_realtime",
                "defender_realtime",
                Status::Critical,
                "Defender real-time protection is off".into(),
                BTreeMap::new(),
                Some("enable_defender_realtime"),
            )]);
        }
        if d.signature_age_days > SIGNATURE_WARN_DAYS {
            return Ok(vec![issue(
                "security.defender_signatures",
                "defender_signatures",
                Status::Warning,
                format!("virus definitions are {} day(s) old", d.signature_age_days),
                BTreeMap::from([("days".to_string(), d.signature_age_days.to_string())]),
                None,
            )]);
        }
        Ok(Vec::new())
    }
}

impl Check for FirewallCheck {
    fn id(&self) -> &'static str {
        "security.firewall"
    }
    fn category(&self) -> Category {
        Category::Security
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        if p.firewall_enabled()? {
            return Ok(Vec::new());
        }
        Ok(vec![issue(
            "security.firewall",
            "firewall",
            Status::Critical,
            "at least one firewall profile is disabled".into(),
            BTreeMap::new(),
            Some("enable_firewall"),
        )])
    }
}

impl Check for UacCheck {
    fn id(&self) -> &'static str {
        "security.uac"
    }
    fn category(&self) -> Category {
        Category::Security
    }
    fn scan(&self, p: &dyn SystemProbe) -> Result<Vec<Issue>, ProbeError> {
        let evidence = match p.uac_level()? {
            UacLevel::Enabled => return Ok(Vec::new()),
            UacLevel::Disabled => "EnableLUA = 0",
            UacLevel::NeverNotify => "ConsentPromptBehaviorAdmin = 0 (never notify)",
        };
        // No automatic fix: changing UAC needs a restart and the user's decision.
        Ok(vec![issue(
            "security.uac",
            "uac",
            Status::Warning,
            evidence.into(),
            BTreeMap::new(),
            None,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::fake::FakeProbe;
    use crate::probe::DefenderState;

    fn def(realtime: bool, age: u32, third: bool) -> FakeProbe {
        FakeProbe {
            defender: Some(DefenderState {
                realtime,
                signature_age_days: age,
                third_party_av: third,
            }),
            ..Default::default()
        }
    }
    fn err() -> ProbeError {
        ProbeError::Unavailable("x".into())
    }

    #[test]
    fn healthy_defender_has_no_issues() {
        assert!(DefenderCheck.scan(&def(true, 0, false)).unwrap().is_empty());
        assert!(DefenderCheck.scan(&def(true, 7, false)).unwrap().is_empty());
    }

    #[test]
    fn realtime_off_is_critical() {
        let i = DefenderCheck.scan(&def(false, 0, false)).unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].status, Status::Critical);
        assert_eq!(i[0].category, Category::Security);
        assert_eq!(i[0].fix.as_deref(), Some("enable_defender_realtime"));
        assert_eq!(i[0].title_key, "issue.security.defender_realtime.title");
    }

    #[test]
    fn signatures_older_than_7_days_is_warning() {
        // Boundary: exactly 7 days is fine, 8 days warns.
        assert!(DefenderCheck.scan(&def(true, 7, false)).unwrap().is_empty());
        let i = DefenderCheck.scan(&def(true, 8, false)).unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].status, Status::Warning);
        assert_eq!(i[0].params["days"], "8");
        assert_eq!(i[0].fix, None);
    }

    #[test]
    fn realtime_off_and_old_signatures_report_only_critical() {
        let i = DefenderCheck.scan(&def(false, 30, false)).unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].status, Status::Critical);
    }

    #[test]
    fn third_party_av_makes_defender_check_unknown_not_critical() {
        let i = DefenderCheck.scan(&def(false, 90, true)).unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].status, Status::Unknown);
        assert_eq!(i[0].id, "security.defender_third_party");
        assert_eq!(i[0].fix, None);
    }

    #[test]
    fn defender_probe_error_is_check_error() {
        let p = FakeProbe {
            defender_error: Some(err()),
            ..Default::default()
        };
        assert_eq!(DefenderCheck.scan(&p), Err(err()));
    }

    #[test]
    fn firewall_disabled_is_critical() {
        let p = FakeProbe {
            firewall: false,
            ..Default::default()
        };
        let i = FirewallCheck.scan(&p).unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].status, Status::Critical);
        assert_eq!(i[0].fix.as_deref(), Some("enable_firewall"));
    }

    #[test]
    fn firewall_enabled_and_error() {
        let on = FakeProbe {
            firewall: true,
            ..Default::default()
        };
        assert!(FirewallCheck.scan(&on).unwrap().is_empty());
        let bad = FakeProbe {
            firewall_error: Some(err()),
            ..Default::default()
        };
        assert_eq!(FirewallCheck.scan(&bad), Err(err()));
    }

    #[test]
    fn uac_disabled_is_warning_without_auto_fix() {
        for level in [UacLevel::Disabled, UacLevel::NeverNotify] {
            let p = FakeProbe {
                uac: Some(level),
                ..Default::default()
            };
            let i = UacCheck.scan(&p).unwrap();
            assert_eq!(i.len(), 1);
            assert_eq!(i[0].status, Status::Warning);
            assert_eq!(i[0].fix, None);
        }
    }

    #[test]
    fn uac_enabled_and_error() {
        assert!(UacCheck.scan(&FakeProbe::default()).unwrap().is_empty());
        let bad = FakeProbe {
            uac_error: Some(err()),
            ..Default::default()
        };
        assert_eq!(UacCheck.scan(&bad), Err(err()));
    }
}

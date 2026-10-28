//! Guided troubleshooters: a pure state machine over probe tests and the fix
//! registry. It carries `FixId`s only; applying a fix goes through the normal
//! preview, confirmation and helper path, one step at a time.

use crate::fixes::FixId;
use crate::probe::{ServiceState, SystemProbe};
use serde::Serialize;
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TroubleshooterId {
    Network,
    WindowsUpdate,
    SystemFiles,
    Slowness,
}

impl TroubleshooterId {
    /// Every troubleshooter, in the order the interface lists them.
    pub const ALL: [TroubleshooterId; 4] = [
        TroubleshooterId::Network,
        TroubleshooterId::WindowsUpdate,
        TroubleshooterId::SystemFiles,
        TroubleshooterId::Slowness,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            TroubleshooterId::Network => "network",
            TroubleshooterId::WindowsUpdate => "windows_update",
            TroubleshooterId::SystemFiles => "system_files",
            TroubleshooterId::Slowness => "slowness",
        }
    }
}

impl FromStr for TroubleshooterId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        TroubleshooterId::ALL
            .into_iter()
            .find(|i| i.as_str() == s)
        .ok_or_else(|| format!("unknown troubleshooter: {}", s.chars().take(32).collect::<String>()))
    }
}

/// Result of one probe test. `Unknown` is never treated as a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Test {
    Gateway,
    Dns,
    Internet,
    /// The three update services exist and none is disabled (stopped is normal: they start on demand).
    UpdateServices,
    /// The pending-update list can be read at all. A read error IS the symptom here, so it fails.
    PendingReadable,
    /// The last successful update install is less than [`UPDATE_STALE_DAYS`] old.
    UpdateRecent,
    /// Every fixed disk has at least 10 % free.
    DiskFree,
    /// The user temp folder is under 1 GiB.
    TempSmall,
    /// Memory use is under 85 %.
    RamOk,
    /// Processor use is under 90 %.
    CpuOk,
}

const UPDATE_STALE_DAYS: u64 = 60;
const UPDATE_SERVICES: [&str; 3] = ["wuauserv", "bits", "cryptsvc"];
const TEMP_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

fn from_bool<E>(r: Result<bool, E>) -> Outcome {
    match r {
        Ok(true) => Outcome::Pass,
        Ok(false) => Outcome::Fail,
        Err(_) => Outcome::Unknown,
    }
}

impl Test {
    pub fn run(self, p: &dyn SystemProbe) -> Outcome {
        match self {
            Test::Gateway => from_bool(p.gateway_reachable()),
            Test::Dns => from_bool(p.dns_resolves()),
            Test::Internet => from_bool(p.internet_reachable()),
            Test::UpdateServices => {
                let mut outcome = Outcome::Pass;
                for name in UPDATE_SERVICES {
                    match p.service_state(name) {
                        Ok(ServiceState::Running | ServiceState::Stopped) => {}
                        Ok(ServiceState::Disabled | ServiceState::Missing) => return Outcome::Fail,
                        Err(_) => outcome = Outcome::Unknown,
                    }
                }
                outcome
            }
            Test::PendingReadable => match p.pending_updates() {
                Ok(_) => Outcome::Pass,
                Err(_) => Outcome::Fail,
            },
            Test::UpdateRecent => match p.last_update_success() {
                Ok(Some(t)) => {
                    let age = p.now().duration_since(t).unwrap_or(Duration::ZERO);
                    if age < Duration::from_secs(UPDATE_STALE_DAYS * 86_400) {
                        Outcome::Pass
                    } else {
                        Outcome::Fail
                    }
                }
                // Never installed / unknown is not evidence of a fault.
                Ok(None) | Err(_) => Outcome::Unknown,
            },
            Test::DiskFree => match p.disks() {
                Ok(disks) => {
                    let fixed: Vec<_> = disks.iter().filter(|d| !d.removable && d.total_bytes > 0).collect();
                    if fixed.is_empty() {
                        Outcome::Unknown
                    } else if fixed.iter().any(|d| d.free_bytes * 10 < d.total_bytes) {
                        Outcome::Fail
                    } else {
                        Outcome::Pass
                    }
                }
                Err(_) => Outcome::Unknown,
            },
            Test::TempSmall => match p.temp_bytes() {
                Ok(b) if b < TEMP_LIMIT_BYTES => Outcome::Pass,
                Ok(_) => Outcome::Fail,
                Err(_) => Outcome::Unknown,
            },
            Test::RamOk => match p.ram_usage() {
                Ok((used, total)) if total > 0 => from_bool::<()>(Ok(used * 100 < total * 85)),
                _ => Outcome::Unknown,
            },
            Test::CpuOk => match p.cpu_percent() {
                Ok(c) => from_bool::<()>(Ok(c < 90.0)),
                Err(_) => Outcome::Unknown,
            },
        }
    }
}

/// One repair, lightest first. `recheck` says whether it worked; `None` when
/// no cheap, unprivileged test exists, in which case the fix's own success counts.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub id: &'static str,
    pub fix: FixId,
    pub recheck: Option<Test>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum State {
    /// Every diagnostic passed: nothing to fix.
    Healthy,
    /// The step at this index is proposed; `unmeasured` = a test could not run.
    Suggest { step: usize, unmeasured: bool },
    /// The recheck passed. `reboot` = the last fix needs a restart to finish.
    Resolved { reboot: bool },
    /// Every step was tried or skipped and the problem remains.
    Exhausted,
}

#[derive(Clone, Copy, Debug)]
pub struct Troubleshooter {
    pub id: TroubleshooterId,
    /// Tests that decide whether a repair is offered. Empty = always offer (no cheap test exists).
    pub diagnose: &'static [Test],
    pub steps: &'static [Step],
    /// Tests whose failure no step can fix: reported as a hint (key) only.
    pub hints: &'static [(Test, &'static str)],
}

const NETWORK_STEPS: [Step; 3] = [
    Step { id: "net.flush_dns", fix: FixId::FlushDns, recheck: Some(Test::Dns) },
    Step { id: "net.renew_ip", fix: FixId::RenewIp, recheck: Some(Test::Internet) },
    Step { id: "net.reset_winsock", fix: FixId::ResetWinsock, recheck: Some(Test::Internet) },
];

const UPDATE_STEPS: [Step; 2] = [
    Step { id: "wu.clean_leftovers", fix: FixId::CleanUpdateLeftovers, recheck: Some(Test::PendingReadable) },
    Step { id: "wu.reset", fix: FixId::ResetWindowsUpdate, recheck: Some(Test::PendingReadable) },
];

const SYSTEM_FILES_STEPS: [Step; 1] =
    [Step { id: "sys.sfc_dism", fix: FixId::RunSfcDism, recheck: None }];

const SLOWNESS_STEPS: [Step; 2] = [
    Step { id: "slow.clean_temp", fix: FixId::CleanTemp, recheck: Some(Test::TempSmall) },
    Step { id: "slow.empty_bin", fix: FixId::EmptyRecycleBin, recheck: Some(Test::DiskFree) },
];

pub fn for_id(id: TroubleshooterId) -> Troubleshooter {
    match id {
        TroubleshooterId::Network => Troubleshooter {
            id,
            diagnose: &[Test::Gateway, Test::Dns, Test::Internet],
            steps: &NETWORK_STEPS,
            hints: &[],
        },
        TroubleshooterId::WindowsUpdate => Troubleshooter {
            id,
            diagnose: &[Test::UpdateServices, Test::PendingReadable, Test::UpdateRecent],
            steps: &UPDATE_STEPS,
            hints: &[],
        },
        TroubleshooterId::SystemFiles => Troubleshooter {
            id,
            diagnose: &[],
            steps: &SYSTEM_FILES_STEPS,
            hints: &[],
        },
        TroubleshooterId::Slowness => Troubleshooter {
            id,
            diagnose: &[Test::DiskFree, Test::TempSmall],
            steps: &SLOWNESS_STEPS,
            hints: &[(Test::RamOk, "high_ram"), (Test::CpuOk, "high_cpu")],
        },
    }
}

pub fn network() -> Troubleshooter {
    for_id(TroubleshooterId::Network)
}

impl Troubleshooter {
    pub fn start(&self, probe: &dyn SystemProbe) -> State {
        if self.diagnose.is_empty() {
            return self.suggest(0, false);
        }
        let outcomes: Vec<Outcome> = self.diagnose.iter().map(|t| t.run(probe)).collect();
        if outcomes.iter().all(|o| *o == Outcome::Pass) {
            return State::Healthy;
        }
        self.suggest(0, outcomes.contains(&Outcome::Unknown))
    }

    /// Keys of the hint tests that definitely failed (a reading that failed is no hint).
    pub fn hints(&self, probe: &dyn SystemProbe) -> Vec<&'static str> {
        self.hints
            .iter()
            .filter(|(t, _)| t.run(probe) == Outcome::Fail)
            .map(|(_, key)| *key)
            .collect()
    }

    /// Call after the fix of `step` ran: re-tests, then resolves or moves on.
    pub fn after_fix(&self, step: usize, probe: &dyn SystemProbe) -> State {
        let Some(s) = self.steps.get(step) else {
            return State::Exhausted;
        };
        let reboot = s.fix == FixId::ResetWinsock;
        match s.recheck.map(|t| t.run(probe)) {
            None | Some(Outcome::Pass) => State::Resolved { reboot },
            Some(o) => self.suggest(step + 1, o == Outcome::Unknown),
        }
    }

    pub fn skip(&self, step: usize) -> State {
        self.suggest(step + 1, false)
    }

    fn suggest(&self, step: usize, unmeasured: bool) -> State {
        if step < self.steps.len() {
            State::Suggest { step, unmeasured }
        } else {
            State::Exhausted
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::fake::FakeProbe;
    use crate::probe::ProbeError;

    fn suggest(step: usize) -> State {
        State::Suggest {
            step,
            unmeasured: false,
        }
    }

    fn dns_broken() -> FakeProbe {
        FakeProbe {
            dns: Some(false),
            ..FakeProbe::default()
        }
    }

    #[test]
    fn healthy_when_all_pass() {
        assert_eq!(network().start(&FakeProbe::default()), State::Healthy);
    }

    #[test]
    fn first_step_resolves() {
        let t = network();
        assert_eq!(t.start(&dns_broken()), suggest(0));
        assert_eq!(t.steps[0].fix, crate::fixes::FixId::FlushDns);
        // The fix worked: DNS resolves again.
        assert_eq!(
            t.after_fix(0, &FakeProbe::default()),
            State::Resolved { reboot: false }
        );
    }

    #[test]
    fn third_step_resolves_needs_reboot() {
        let t = network();
        assert_eq!(t.steps[2].fix, crate::fixes::FixId::ResetWinsock);
        assert_eq!(
            t.after_fix(2, &FakeProbe::default()),
            State::Resolved { reboot: true }
        );
    }

    #[test]
    fn exhausted_after_last_step() {
        let t = network();
        assert_eq!(t.after_fix(2, &dns_broken_internet()), State::Exhausted);
    }

    fn dns_broken_internet() -> FakeProbe {
        FakeProbe {
            internet: Some(false),
            ..FakeProbe::default()
        }
    }

    #[test]
    fn unknown_is_not_healthy() {
        let err = || Some(ProbeError::Unavailable("x".into()));
        let p = FakeProbe {
            gateway_error: err(),
            dns_error: err(),
            internet_error: err(),
            ..FakeProbe::default()
        };
        assert_eq!(
            network().start(&p),
            State::Suggest {
                step: 0,
                unmeasured: true
            }
        );
    }

    #[test]
    fn skip_moves_on_and_exhausts() {
        let t = network();
        assert_eq!(t.skip(0), suggest(1));
        assert_eq!(t.skip(2), State::Exhausted);
    }

    #[test]
    fn no_step_is_suggested_twice() {
        let t = network();
        let p = FakeProbe {
            internet: Some(false),
            dns: Some(false),
            ..FakeProbe::default()
        };
        assert_eq!(t.after_fix(0, &p), suggest(1));
        assert_eq!(t.after_fix(1, &p), suggest(2));
        assert_eq!(t.after_fix(2, &p), State::Exhausted);
    }

    #[test]
    fn no_gateway_still_starts_with_the_lightest_step() {
        let p = FakeProbe {
            gateway: Some(false),
            internet: Some(false),
            ..FakeProbe::default()
        };
        assert_eq!(network().start(&p), suggest(0));
    }

    #[test]
    fn a_step_past_the_end_is_exhausted_not_a_panic() {
        assert_eq!(
            network().after_fix(9, &FakeProbe::default()),
            State::Exhausted
        );
    }

    // ---- more troubleshooters -------------------------------------------------

    use crate::probe::{DiskInfo, ServiceState};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(200 * 86_400 * 10)
    }

    fn update_ok() -> FakeProbe {
        let running = |n: &str| (n.to_string(), ServiceState::Running);
        FakeProbe {
            services: [running("wuauserv"), running("bits"), running("cryptsvc")].into(),
            last_update: Some(now() - Duration::from_secs(86_400)),
            now: Some(now()),
            ..FakeProbe::default()
        }
    }

    fn disk(free_pct: u64) -> DiskInfo {
        DiskInfo { mount: "C:".into(), total_bytes: 100 * GIB, free_bytes: free_pct * GIB, removable: false }
    }

    fn slow_ok() -> FakeProbe {
        FakeProbe { disks: vec![disk(50)], temp_bytes: GIB / 10, ..FakeProbe::default() }
    }

    #[test]
    fn ids_parse_and_every_id_has_a_troubleshooter() {
        for (name, steps) in [("network", 3), ("windows_update", 2), ("system_files", 1), ("slowness", 2)] {
            let id: TroubleshooterId = name.parse().unwrap();
            assert_eq!(id.as_str(), name);
            assert_eq!(for_id(id).steps.len(), steps, "{name}");
        }
        assert!("nope".parse::<TroubleshooterId>().is_err());
    }

    #[test]
    fn update_is_healthy_when_services_exist_and_the_last_install_is_recent() {
        assert_eq!(for_id(TroubleshooterId::WindowsUpdate).start(&update_ok()), State::Healthy);
    }

    #[test]
    fn update_disabled_service_or_old_install_starts_with_the_leftovers_step() {
        let t = for_id(TroubleshooterId::WindowsUpdate);
        assert_eq!(t.steps[0].fix, crate::fixes::FixId::CleanUpdateLeftovers);
        assert_eq!(t.steps[1].fix, crate::fixes::FixId::ResetWindowsUpdate);
        let mut p = update_ok();
        p.services.insert("bits".into(), ServiceState::Disabled);
        assert_eq!(t.start(&p), suggest(0));
        let old = FakeProbe { last_update: Some(now() - Duration::from_secs(90 * 86_400)), ..update_ok() };
        assert_eq!(t.start(&old), suggest(0));
    }

    #[test]
    fn unreadable_pending_updates_is_the_symptom_not_an_unknown() {
        let p = FakeProbe { pending_updates_error: Some(ProbeError::Unavailable("x".into())), ..update_ok() };
        assert_eq!(Test::PendingReadable.run(&p), Outcome::Fail);
    }

    #[test]
    fn system_files_has_no_cheap_test_so_it_always_offers_its_one_step() {
        let t = for_id(TroubleshooterId::SystemFiles);
        assert_eq!(t.steps[0].fix, crate::fixes::FixId::RunSfcDism);
        assert!(t.steps[0].recheck.is_none());
        assert_eq!(t.start(&FakeProbe::default()), suggest(0));
        // Without a re-test the fix's own success is the answer.
        assert_eq!(t.after_fix(0, &FakeProbe::default()), State::Resolved { reboot: false });
        assert_eq!(t.skip(0), State::Exhausted);
    }

    #[test]
    fn slowness_is_healthy_with_room_on_disk_and_a_small_temp() {
        assert_eq!(for_id(TroubleshooterId::Slowness).start(&slow_ok()), State::Healthy);
    }

    #[test]
    fn slowness_low_disk_or_big_temp_starts_with_cleaning_temp() {
        let t = for_id(TroubleshooterId::Slowness);
        assert_eq!(t.steps[0].fix, crate::fixes::FixId::CleanTemp);
        assert_eq!(t.steps[1].fix, crate::fixes::FixId::EmptyRecycleBin);
        let low = FakeProbe { disks: vec![disk(5)], ..slow_ok() };
        assert_eq!(t.start(&low), suggest(0));
        let big = FakeProbe { temp_bytes: 2 * GIB, ..slow_ok() };
        assert_eq!(t.start(&big), suggest(0));
        // Cleaning temp worked: the temp test passes again.
        assert_eq!(t.after_fix(0, &slow_ok()), State::Resolved { reboot: false });
    }

    #[test]
    fn removable_and_missing_disks_do_not_count() {
        let usb = DiskInfo { removable: true, ..disk(1) };
        assert_eq!(Test::DiskFree.run(&FakeProbe { disks: vec![disk(50), usb], ..FakeProbe::default() }), Outcome::Pass);
        assert_eq!(Test::DiskFree.run(&FakeProbe::default()), Outcome::Unknown);
    }

    #[test]
    fn busy_memory_and_processor_are_hints_not_steps() {
        let t = for_id(TroubleshooterId::Slowness);
        assert_eq!(t.hints(&slow_ok()), Vec::<&str>::new());
        let busy = FakeProbe { ram_used: 90, ram_total: 100, cpu_percent: 95.0, ..slow_ok() };
        assert_eq!(t.hints(&busy), vec!["high_ram", "high_cpu"]);
        // A failed reading is not a hint.
        let broken = FakeProbe { ram_error: Some(ProbeError::Unavailable("x".into())), ..slow_ok() };
        assert!(t.hints(&broken).is_empty());
        // Hints do not turn a healthy result into a repair.
        assert_eq!(t.start(&busy), State::Healthy);
    }

    #[test]
    fn ids_match_the_interface_list() {
        let api = include_str!("../../../ui/src/api.ts");
        let line = api
            .lines()
            .find(|l| l.starts_with("export const TROUBLESHOOTERS"))
            .expect("TROUBLESHOOTERS in api.ts");
        let listed: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        let ours: Vec<&str> = TroubleshooterId::ALL.iter().map(|i| i.as_str()).collect();
        assert_eq!(listed, ours);
    }

    #[test]
    fn every_troubleshooter_has_its_texts_in_tr_json() {
        let tr: serde_json::Value =
            serde_json::from_str(include_str!("../../../ui/src/i18n/tr.json")).unwrap();
        let has = |key: &str| {
            let mut v = &tr;
            for part in key.split('.') {
                v = v.get(part).unwrap_or(&serde_json::Value::Null);
            }
            v.as_str().is_some_and(|s| !s.trim().is_empty())
        };
        for id in TroubleshooterId::ALL {
            let n = id.as_str();
            for key in [
                format!("ui.troubleshoot.titles.{n}"),
                format!("ui.troubleshoot.descriptions.{n}"),
                format!("ui.troubleshoot.texts.{n}.healthy"),
                format!("ui.troubleshoot.texts.{n}.resolved"),
                format!("ui.troubleshoot.texts.{n}.exhausted"),
            ] {
                assert!(has(&key), "missing {key}");
            }
            for step in for_id(id).steps {
                let key = format!("ui.troubleshoot.why.{}", step.fix.as_str());
                assert!(has(&key), "missing {key}");
            }
        }
    }
}

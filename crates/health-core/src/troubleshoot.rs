//! Guided troubleshooters: a pure state machine over probe tests and the fix
//! registry. It carries `FixId`s only; applying a fix goes through the normal
//! preview, confirmation and helper path, one step at a time.

use crate::fixes::FixId;
use crate::probe::SystemProbe;
use serde::Serialize;

/// Result of one probe test. `Unknown` is never treated as a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetTest {
    Gateway,
    Dns,
    Internet,
}

impl NetTest {
    pub fn run(self, probe: &dyn SystemProbe) -> Outcome {
        let r = match self {
            NetTest::Gateway => probe.gateway_reachable(),
            NetTest::Dns => probe.dns_resolves(),
            NetTest::Internet => probe.internet_reachable(),
        };
        match r {
            Ok(true) => Outcome::Pass,
            Ok(false) => Outcome::Fail,
            Err(_) => Outcome::Unknown,
        }
    }
}

/// One repair, lightest first, and the test that says whether it worked.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub id: &'static str,
    pub fix: FixId,
    pub recheck: NetTest,
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
    pub diagnose: &'static [NetTest],
    pub steps: &'static [Step],
}

const NETWORK_STEPS: [Step; 3] = [
    Step {
        id: "net.flush_dns",
        fix: FixId::FlushDns,
        recheck: NetTest::Dns,
    },
    Step {
        id: "net.renew_ip",
        fix: FixId::RenewIp,
        recheck: NetTest::Internet,
    },
    Step {
        id: "net.reset_winsock",
        fix: FixId::ResetWinsock,
        recheck: NetTest::Internet,
    },
];

pub fn network() -> Troubleshooter {
    Troubleshooter {
        diagnose: &[NetTest::Gateway, NetTest::Dns, NetTest::Internet],
        steps: &NETWORK_STEPS,
    }
}

impl Troubleshooter {
    pub fn start(&self, probe: &dyn SystemProbe) -> State {
        let outcomes: Vec<Outcome> = self.diagnose.iter().map(|t| t.run(probe)).collect();
        if outcomes.iter().all(|o| *o == Outcome::Pass) {
            return State::Healthy;
        }
        self.suggest(0, outcomes.contains(&Outcome::Unknown))
    }

    /// Call after the fix of `step` ran: re-tests, then resolves or moves on.
    pub fn after_fix(&self, step: usize, probe: &dyn SystemProbe) -> State {
        let Some(s) = self.steps.get(step) else {
            return State::Exhausted;
        };
        match s.recheck.run(probe) {
            Outcome::Pass => State::Resolved {
                reboot: s.fix == FixId::ResetWinsock,
            },
            o => self.suggest(step + 1, o == Outcome::Unknown),
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
}

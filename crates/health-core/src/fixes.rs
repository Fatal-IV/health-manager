//! Fix registry: the closed set of repairs the app can perform, their
//! preview plans and the argument validation shared by the Tauri command
//! and the elevated helper (a security boundary).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FixId {
    CleanTemp,
    EmptyRecycleBin,
    CleanUpdateLeftovers,
    /// Arg: startup id `HKCU|name` / `HKLM|name`. Helper contract: the value
    /// is DATA only; re-enumerate the real Run entries (64-bit and 32-bit
    /// registry views) and act only on an exact id match. Never build a
    /// registry path, file path or command line from it, and never pass it
    /// to a shell.
    DisableStartupItem,
    /// Arg: numeric pid. Helper contract: refuse protected/critical processes
    /// and verify the pid still maps to a running process (Task 10).
    EndProcess,
    ResetWindowsUpdate,
    EnableDefenderRealtime,
    EnableFirewall,
    RunSfcDism,
    /// Read-only: reads the boot duration from the event log, which Windows only
    /// shows to administrators. Changes nothing on the system.
    ReadBootTime,
    /// Clears the DNS resolver cache. `ipconfig /flushdns` refuses to run unelevated.
    FlushDns,
    /// Releases and renews the IP lease; the connection drops for a moment.
    RenewIp,
    /// Resets the Winsock catalog; a restart is needed to finish.
    ResetWinsock,
    /// Restarts the Windows audio services (`Audiosrv`, `AudioEndpointBuilder`).
    RestartAudioServices,
    /// Restarts the print spooler service (`Spooler`).
    RestartSpooler,
    /// Stops the spooler, deletes the queued print jobs (`.SPL`/`.SHD`), starts it again.
    ClearPrintQueue,
    /// Restarts the `bthserv` service.
    RestartBluetooth,
    /// Restarts the `FrameServer` service.
    RestartCameraService,
}

impl FixId {
    pub const ALL: [FixId; 18] = [
        FixId::CleanTemp,
        FixId::EmptyRecycleBin,
        FixId::CleanUpdateLeftovers,
        FixId::DisableStartupItem,
        FixId::EndProcess,
        FixId::ResetWindowsUpdate,
        FixId::EnableDefenderRealtime,
        FixId::EnableFirewall,
        FixId::RunSfcDism,
        FixId::ReadBootTime,
        FixId::FlushDns,
        FixId::RenewIp,
        FixId::ResetWinsock,
        FixId::RestartAudioServices,
        FixId::RestartSpooler,
        FixId::ClearPrintQueue,
        FixId::RestartBluetooth,
        FixId::RestartCameraService,
    ];

    /// Exhaustive (no wildcard): a new variant fails to compile until it is
    /// given an index here. The tests check that `ALL` is in index order and
    /// contains every variant of an independent exhaustive list.
    pub const fn index(self) -> usize {
        match self {
            FixId::CleanTemp => 0,
            FixId::EmptyRecycleBin => 1,
            FixId::CleanUpdateLeftovers => 2,
            FixId::DisableStartupItem => 3,
            FixId::EndProcess => 4,
            FixId::ResetWindowsUpdate => 5,
            FixId::EnableDefenderRealtime => 6,
            FixId::EnableFirewall => 7,
            FixId::RunSfcDism => 8,
            FixId::ReadBootTime => 9,
            FixId::FlushDns => 10,
            FixId::RenewIp => 11,
            FixId::ResetWinsock => 12,
            FixId::RestartAudioServices => 13,
            FixId::RestartSpooler => 14,
            FixId::ClearPrintQueue => 15,
            FixId::RestartBluetooth => 16,
            FixId::RestartCameraService => 17,
        }
    }

    /// Exhaustive: every variant declares whether it requires an argument.
    pub const fn takes_arg(self) -> bool {
        match self {
            FixId::DisableStartupItem | FixId::EndProcess => true,
            FixId::CleanTemp
            | FixId::EmptyRecycleBin
            | FixId::CleanUpdateLeftovers
            | FixId::ResetWindowsUpdate
            | FixId::EnableDefenderRealtime
            | FixId::EnableFirewall
            | FixId::RunSfcDism
            | FixId::ReadBootTime
            | FixId::FlushDns
            | FixId::RenewIp
            | FixId::ResetWinsock
            | FixId::RestartAudioServices
            | FixId::RestartSpooler
            | FixId::ClearPrintQueue
            | FixId::RestartBluetooth
            | FixId::RestartCameraService => false,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FixId::CleanTemp => "clean_temp",
            FixId::EmptyRecycleBin => "empty_recycle_bin",
            FixId::CleanUpdateLeftovers => "clean_update_leftovers",
            FixId::DisableStartupItem => "disable_startup_item",
            FixId::EndProcess => "end_process",
            FixId::ResetWindowsUpdate => "reset_windows_update",
            FixId::EnableDefenderRealtime => "enable_defender_realtime",
            FixId::EnableFirewall => "enable_firewall",
            FixId::RunSfcDism => "run_sfc_dism",
            FixId::ReadBootTime => "read_boot_time",
            FixId::FlushDns => "flush_dns",
            FixId::RenewIp => "renew_ip",
            FixId::ResetWinsock => "reset_winsock",
            FixId::RestartAudioServices => "restart_audio_services",
            FixId::RestartSpooler => "restart_spooler",
            FixId::ClearPrintQueue => "clear_print_queue",
            FixId::RestartBluetooth => "restart_bluetooth",
            FixId::RestartCameraService => "restart_camera_service",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FixError {
    #[error("unknown fix id: {0}")]
    UnknownFix(String),
    #[error("invalid argument for {fix}: {reason}")]
    InvalidArg {
        fix: &'static str,
        reason: &'static str,
    },
}

impl FromStr for FixId {
    type Err = FixError;
    fn from_str(s: &str) -> Result<Self, FixError> {
        FixId::ALL
            .into_iter()
            .find(|f| f.as_str() == s)
            .ok_or_else(|| FixError::UnknownFix(s.chars().take(64).collect()))
    }
}

impl Serialize for FixId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FixId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Preview shown to the user before a fix runs. All text fields are i18n keys;
/// values that cannot be embedded in prose go to `params` (e.g. `target`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FixPlan {
    pub fix: FixId,
    pub summary_key: String,
    pub changes: Vec<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    pub needs_admin: bool,
    pub reversible: bool,
}

/// Number of change lines per fix (`fix.<id>.change.1` ..= `.n`).
fn change_count(fix: FixId) -> usize {
    match fix {
        FixId::CleanTemp => 2,
        FixId::EmptyRecycleBin => 1,
        FixId::CleanUpdateLeftovers => 1,
        FixId::DisableStartupItem => 2,
        FixId::EndProcess => 2,
        FixId::ResetWindowsUpdate => 3,
        FixId::EnableDefenderRealtime => 1,
        FixId::EnableFirewall => 1,
        FixId::RunSfcDism => 2,
        FixId::ReadBootTime => 2,
        FixId::FlushDns => 1,
        FixId::RenewIp => 2,
        FixId::ResetWinsock => 2,
        FixId::RestartAudioServices => 2,
        FixId::RestartSpooler => 2,
        FixId::ClearPrintQueue => 3,
        FixId::RestartBluetooth => 2,
        FixId::RestartCameraService => 2,
    }
}

/// Fixed by the spec: only machine-wide repairs need the elevated helper.
fn needs_admin(fix: FixId) -> bool {
    matches!(
        fix,
        FixId::CleanUpdateLeftovers
            | FixId::ResetWindowsUpdate
            | FixId::EnableDefenderRealtime
            | FixId::EnableFirewall
            | FixId::RunSfcDism
            | FixId::ReadBootTime
            | FixId::FlushDns
            | FixId::RenewIp
            | FixId::ResetWinsock
            | FixId::RestartAudioServices
            | FixId::RestartSpooler
            | FixId::ClearPrintQueue
            | FixId::RestartBluetooth
            | FixId::RestartCameraService
    )
}

/// Reversible = the user can get back to the previous state: startup items
/// can be re-enabled, the Windows Update folders are kept as `.bak`, and
/// Defender/firewall can be switched off again. Deleted files, ended
/// processes and SFC/DISM repairs cannot be undone.
fn reversible(fix: FixId) -> bool {
    matches!(
        fix,
        FixId::DisableStartupItem
            | FixId::ResetWindowsUpdate
            | FixId::EnableDefenderRealtime
            | FixId::EnableFirewall
            | FixId::ReadBootTime
    )
}

/// Builds the preview for `fix`. Call [`validate_arg`] first for untrusted input.
pub fn fix_plan(fix: FixId, arg: Option<&str>) -> FixPlan {
    let id = fix.as_str();
    let mut params = BTreeMap::new();
    if let Some(a) = arg {
        if matches!(fix, FixId::DisableStartupItem | FixId::EndProcess) {
            params.insert("target".to_string(), a.to_string());
        }
    }
    FixPlan {
        fix,
        summary_key: format!("fix.{id}.summary"),
        changes: (1..=change_count(fix))
            .map(|n| format!("fix.{id}.change.{n}"))
            .collect(),
        params,
        needs_admin: needs_admin(fix),
        reversible: reversible(fix),
    }
}

const MAX_ARG_LEN: usize = 512;

/// Argument contract check. Must run before planning or applying a fix.
///
/// This is a contract, not character filtering: for `DisableStartupItem` the
/// value may contain `\` and `/` (legal in registry value names). The elevated
/// helper (Task 10) must treat the value as DATA only: re-enumerate the real
/// Run entries in both the 64-bit and 32-bit registry views and act only on an
/// exact id match; never build a registry path, file path or command line from
/// it, and never pass it to a shell. For `EndProcess` the helper must also
/// refuse protected/critical processes and verify the pid still maps to a
/// running process.
pub fn validate_arg(fix: FixId, arg: Option<&str>) -> Result<(), FixError> {
    let bad = |reason| {
        Err(FixError::InvalidArg {
            fix: fix.as_str(),
            reason,
        })
    };
    match fix {
        FixId::DisableStartupItem => {
            let Some(a) = arg else {
                return bad("argument required");
            };
            if a.len() > MAX_ARG_LEN {
                return bad("too long");
            }
            if a.chars().any(|c| c.is_control()) {
                return bad("control characters");
            }
            match a.split_once('|') {
                Some(("HKCU" | "HKLM", name)) if !name.trim().is_empty() => Ok(()),
                _ => bad("expected HKCU|name or HKLM|name"),
            }
        }
        FixId::EndProcess => {
            let Some(a) = arg else {
                return bad("argument required");
            };
            if a.is_empty() || a.len() > 10 || !a.bytes().all(|b| b.is_ascii_digit()) {
                return bad("pid must be digits only");
            }
            match a.parse::<u32>() {
                Ok(pid) if pid > 4 => Ok(()),
                _ => bad("pid out of range"),
            }
        }
        _ => match arg {
            None => Ok(()),
            Some(_) => bad("this fix takes no argument"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::all_checks;
    use crate::probe::fake::FakeProbe;

    fn takes_arg(f: FixId) -> bool {
        f.takes_arg()
    }

    /// Exhaustive, no wildcard: adding a variant breaks this match, forcing
    /// `expected_variants` below to be updated too.
    #[allow(clippy::needless_match)] // the exhaustive match is the point
    fn witness(f: FixId) -> FixId {
        match f {
            FixId::CleanTemp => FixId::CleanTemp,
            FixId::EmptyRecycleBin => FixId::EmptyRecycleBin,
            FixId::CleanUpdateLeftovers => FixId::CleanUpdateLeftovers,
            FixId::DisableStartupItem => FixId::DisableStartupItem,
            FixId::EndProcess => FixId::EndProcess,
            FixId::ResetWindowsUpdate => FixId::ResetWindowsUpdate,
            FixId::EnableDefenderRealtime => FixId::EnableDefenderRealtime,
            FixId::EnableFirewall => FixId::EnableFirewall,
            FixId::RunSfcDism => FixId::RunSfcDism,
            FixId::ReadBootTime => FixId::ReadBootTime,
            FixId::FlushDns => FixId::FlushDns,
            FixId::RenewIp => FixId::RenewIp,
            FixId::ResetWinsock => FixId::ResetWinsock,
            FixId::RestartAudioServices => FixId::RestartAudioServices,
            FixId::RestartSpooler => FixId::RestartSpooler,
            FixId::ClearPrintQueue => FixId::ClearPrintQueue,
            FixId::RestartBluetooth => FixId::RestartBluetooth,
            FixId::RestartCameraService => FixId::RestartCameraService,
        }
    }

    /// Hand-written list, independent of `FixId::ALL`.
    fn expected_variants() -> Vec<FixId> {
        [
            FixId::CleanTemp,
            FixId::EmptyRecycleBin,
            FixId::CleanUpdateLeftovers,
            FixId::DisableStartupItem,
            FixId::EndProcess,
            FixId::ResetWindowsUpdate,
            FixId::EnableDefenderRealtime,
            FixId::EnableFirewall,
            FixId::RunSfcDism,
            FixId::ReadBootTime,
            FixId::FlushDns,
            FixId::RenewIp,
            FixId::ResetWinsock,
            FixId::RestartAudioServices,
            FixId::RestartSpooler,
            FixId::ClearPrintQueue,
            FixId::RestartBluetooth,
            FixId::RestartCameraService,
        ]
        .into_iter()
        .map(witness)
        .collect()
    }

    fn missing_from(all: &[FixId], expected: &[FixId]) -> Vec<FixId> {
        expected
            .iter()
            .copied()
            .filter(|e| !all.contains(e))
            .collect()
    }

    #[test]
    fn all_is_complete_and_distinct() {
        let expected = expected_variants();
        assert_eq!(missing_from(&FixId::ALL, &expected), vec![]);
        assert_eq!(FixId::ALL.len(), expected.len());
        for (i, f) in FixId::ALL.iter().enumerate() {
            assert_eq!(f.index(), i, "{f:?} out of order or duplicated");
        }
    }

    #[test]
    fn new_network_fixes_have_expected_plans() {
        let flush = fix_plan(FixId::FlushDns, None);
        assert!(flush.needs_admin && !flush.reversible);
        assert_eq!(flush.changes.len(), 1);
        let renew = fix_plan(FixId::RenewIp, None);
        assert!(renew.needs_admin && !renew.reversible);
        assert_eq!(renew.changes.len(), 2);
        let winsock = fix_plan(FixId::ResetWinsock, None);
        assert!(winsock.needs_admin && !winsock.reversible);
        assert_eq!(winsock.changes.len(), 2);
        assert!(validate_arg(FixId::RenewIp, Some("x")).is_err());
    }

    #[test]
    fn missing_from_detects_a_removed_entry() {
        let expected = expected_variants();
        let mut all = FixId::ALL.to_vec();
        all.retain(|f| *f != FixId::RunSfcDism);
        assert_eq!(missing_from(&all, &expected), vec![FixId::RunSfcDism]);
    }

    #[test]
    fn every_fix_id_roundtrips_through_str() {
        for f in FixId::ALL {
            assert_eq!(f.as_str().parse::<FixId>(), Ok(f));
        }
    }

    #[test]
    fn unknown_fix_id_is_rejected() {
        for bad in ["", "nope", "CleanTemp", "clean_temp ", "format_c"] {
            assert!(
                matches!(bad.parse::<FixId>(), Err(FixError::UnknownFix(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn serde_uses_snake_case_strings() {
        for f in FixId::ALL {
            let json = serde_json::to_string(&f).unwrap();
            assert_eq!(json, format!("\"{}\"", f.as_str()));
            assert_eq!(serde_json::from_str::<FixId>(&json).unwrap(), f);
        }
        assert!(serde_json::from_str::<FixId>("\"bogus\"").is_err());
    }

    #[test]
    fn admin_requirements_match_spec() {
        use FixId::*;
        for f in [CleanTemp, EmptyRecycleBin, DisableStartupItem, EndProcess] {
            assert!(!fix_plan(f, None).needs_admin, "{f:?}");
        }
        for f in [
            CleanUpdateLeftovers,
            ResetWindowsUpdate,
            EnableDefenderRealtime,
            EnableFirewall,
            RunSfcDism,
        ] {
            assert!(fix_plan(f, None).needs_admin, "{f:?}");
        }
    }

    #[test]
    fn reversibility_is_as_documented() {
        use FixId::*;
        for f in [
            DisableStartupItem,
            ResetWindowsUpdate,
            EnableDefenderRealtime,
            EnableFirewall,
        ] {
            assert!(fix_plan(f, None).reversible, "{f:?}");
        }
        for f in [
            CleanTemp,
            EmptyRecycleBin,
            CleanUpdateLeftovers,
            EndProcess,
            RunSfcDism,
        ] {
            assert!(!fix_plan(f, None).reversible, "{f:?}");
        }
    }

    #[test]
    fn device_fixes_have_expected_plans() {
        for (f, changes) in [
            (FixId::RestartAudioServices, 2),
            (FixId::RestartSpooler, 2),
            (FixId::ClearPrintQueue, 3),
            (FixId::RestartBluetooth, 2),
            (FixId::RestartCameraService, 2),
        ] {
            let p = fix_plan(f, None);
            assert!(p.needs_admin, "{f:?}");
            assert!(!p.reversible, "{f:?}");
            assert_eq!(p.changes.len(), changes, "{f:?}");
            assert!(validate_arg(f, Some("x")).is_err(), "{f:?}");
        }
    }

    #[test]
    fn plan_lists_at_least_one_change() {
        for f in FixId::ALL {
            let p = fix_plan(f, None);
            assert!(!p.changes.is_empty(), "{f:?}");
            assert_eq!(p.summary_key, format!("fix.{}.summary", f.as_str()));
            assert_eq!(p.fix, f);
        }
    }

    #[test]
    fn plan_carries_target_param() {
        let p = fix_plan(FixId::EndProcess, Some("1234"));
        assert_eq!(p.params.get("target").map(String::as_str), Some("1234"));
        assert!(fix_plan(FixId::CleanTemp, None).params.is_empty());
    }

    #[test]
    fn every_plan_key_exists_in_tr_json() {
        let tr: serde_json::Value =
            serde_json::from_str(include_str!("../../../ui/src/i18n/tr.json")).unwrap();
        let lookup = |key: &str| {
            let mut v = &tr;
            for part in key.split('.') {
                v = v.get(part).unwrap_or(&serde_json::Value::Null);
            }
            v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)
        };
        for f in FixId::ALL {
            let p = fix_plan(f, None);
            assert!(lookup(&p.summary_key), "missing {}", p.summary_key);
            for c in &p.changes {
                assert!(lookup(c), "missing {c}");
            }
        }
    }

    #[test]
    fn validate_arg_accepts_good_values() {
        assert!(validate_arg(FixId::DisableStartupItem, Some("HKCU|OneDrive")).is_ok());
        assert!(validate_arg(FixId::DisableStartupItem, Some("HKLM|Adobe Updater")).is_ok());
        assert!(validate_arg(FixId::DisableStartupItem, Some("HKCU|Foo/Bar")).is_ok());
        assert!(validate_arg(FixId::DisableStartupItem, Some(r"HKLM|a\b")).is_ok());
        assert!(validate_arg(FixId::EndProcess, Some("5")).is_ok());
        assert!(validate_arg(FixId::EndProcess, Some("4294967295")).is_ok());
        for f in FixId::ALL.into_iter().filter(|f| !takes_arg(*f)) {
            assert!(validate_arg(f, None).is_ok(), "{f:?}");
        }
    }

    #[test]
    fn validate_arg_requires_arg_where_needed() {
        for f in FixId::ALL {
            assert_eq!(validate_arg(f, None).is_err(), takes_arg(f), "{f:?}");
        }
    }

    #[test]
    fn validate_arg_rejects_stray_arg() {
        for f in FixId::ALL.into_iter().filter(|f| !takes_arg(*f)) {
            assert!(validate_arg(f, Some("x")).is_err(), "{f:?}");
            assert!(validate_arg(f, Some("")).is_err(), "{f:?}");
        }
    }

    #[test]
    fn validate_arg_rejects_hostile_inputs() {
        let big = "a".repeat(10 * 1024);
        let big_startup = format!("HKCU|{big}");
        let hostile = [
            "x & calc",
            "..\\..\\x",
            "",
            big.as_str(),
            big_startup.as_str(),
            "12abc",
            "-5",
            "+7",
            "0",
            "4",
            "1",
            "99999999999",
            " 100",
            "HKCU|",
            "HKCU|a\nb",
            "HKCU|a\0b",
            "HKXX|a",
        ];
        for h in hostile {
            for f in [FixId::DisableStartupItem, FixId::EndProcess] {
                assert!(
                    validate_arg(f, Some(h)).is_err(),
                    "{f:?} accepted {:?}",
                    h.chars().take(20).collect::<String>()
                );
            }
        }
    }

    #[test]
    fn issues_from_all_checks_carry_valid_fix_ids() {
        let p = FakeProbe::default();
        for c in all_checks() {
            if let Ok(issues) = c.scan(&p) {
                for i in issues {
                    if let Some(f) = i.fix {
                        assert_eq!(f.as_str().parse::<FixId>(), Ok(f));
                    }
                }
            }
        }
    }
}

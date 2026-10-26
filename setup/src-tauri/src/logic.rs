//! Pure decisions of the installer (no file system, no registry), so they can be tested.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Product {
    pub name: String,
    pub slug: String,
    pub publisher: String,
    pub description: String,
    pub exe: String,
    pub helper: String,
    pub folder: String,
}

/// `theme` and `texts` are handed to the window as they are, so they can be changed
/// in `setup.config.json` without touching any code.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Config {
    pub product: Product,
    pub theme: serde_json::Value,
    pub texts: serde_json::Value,
}

pub fn config() -> Config {
    serde_json::from_str(include_str!("../../setup.config.json")).expect("setup.config.json is valid")
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Install,
    Update,
    Reinstall,
    Uninstall,
}

/// `1.2.3` (a leading `v` is allowed), exactly three numeric parts.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.trim().trim_start_matches('v').split('.');
    let v = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    it.next().is_none().then_some(v)
}

/// Which screen flow applies, and whether it starts by itself (an older install is simply updated).
/// `--uninstall` only counts when this program runs as the copy named `uninstall.exe`.
pub fn decide_mode(args: &[String], own_name: &str, installed: Option<&str>, payload: &str) -> (Mode, bool) {
    if args.iter().any(|a| a == "--uninstall") && own_name.eq_ignore_ascii_case("uninstall.exe") {
        return (Mode::Uninstall, false);
    }
    match installed.and_then(parse_version) {
        None => (Mode::Install, false),
        Some(v) => match parse_version(payload) {
            Some(p) if v < p => (Mode::Update, true),
            _ => (Mode::Reinstall, false),
        },
    }
}

/// The folder to remove, or `None` when this does not look like our own installed copy.
/// Guards against a stray `uninstall.exe` deleting some unrelated folder.
pub fn uninstall_dir(own_exe: &Path, product: &Product) -> Option<PathBuf> {
    let name = own_exe.file_name()?.to_str()?;
    if !name.eq_ignore_ascii_case("uninstall.exe") {
        return None;
    }
    let dir = own_exe.parent()?;
    let has_app = dir.join(&product.exe).is_file();
    let right_folder = dir.file_name()?.to_str()? == product.folder;
    (has_app && right_folder).then(|| dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn config_parses_and_has_every_mode_text() {
        let c = config();
        assert_eq!(c.product.exe, "saglik-denetimi.exe");
        for m in ["install", "update", "reinstall", "uninstall"] {
            for k in ["title", "sub", "button", "terms", "working", "done_title", "done_text", "done_button"] {
                assert!(c.texts[m][k].is_string(), "texts.{m}.{k} missing");
            }
        }
        for k in ["title", "retry", "locked", "generic"] {
            assert!(c.texts["errors"][k].is_string(), "texts.errors.{k} missing");
        }
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
        assert_eq!(parse_version("1.2"), None);
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    }

    #[test]
    fn fresh_machine_asks_before_installing() {
        assert_eq!(decide_mode(&args(&[]), "setup.exe", None, "0.3.0"), (Mode::Install, false));
        // The updater passes --update, but with nothing installed the user still decides.
        assert_eq!(decide_mode(&args(&["--update"]), "setup.exe", None, "0.3.0"), (Mode::Install, false));
    }

    #[test]
    fn older_install_is_updated_at_once() {
        assert_eq!(decide_mode(&args(&[]), "setup.exe", Some("0.2.5"), "0.3.0"), (Mode::Update, true));
    }

    #[test]
    fn same_or_newer_install_offers_reinstall_without_starting() {
        assert_eq!(decide_mode(&args(&[]), "setup.exe", Some("0.3.0"), "0.3.0"), (Mode::Reinstall, false));
        assert_eq!(decide_mode(&args(&[]), "setup.exe", Some("0.4.0"), "0.3.0"), (Mode::Reinstall, false));
    }

    #[test]
    fn uninstall_needs_the_flag_and_the_right_file_name() {
        assert_eq!(decide_mode(&args(&["--uninstall"]), "uninstall.exe", Some("0.3.0"), "0.3.0"), (Mode::Uninstall, false));
        assert_eq!(decide_mode(&args(&["--uninstall"]), "Setup.exe", Some("0.3.0"), "0.3.0").0, Mode::Reinstall);
        assert_eq!(decide_mode(&args(&[]), "uninstall.exe", None, "0.3.0").0, Mode::Install);
    }

    #[test]
    fn uninstall_dir_only_for_our_own_installed_copy() {
        let p = config().product;
        let root = std::env::temp_dir().join(format!("sd-setup-test-{}", std::process::id()));
        let ours = root.join(&p.folder);
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::write(ours.join(&p.exe), b"x").unwrap();
        assert_eq!(uninstall_dir(&ours.join("uninstall.exe"), &p), Some(ours.clone()));
        // Wrong file name.
        assert_eq!(uninstall_dir(&ours.join("setup.exe"), &p), None);
        // Right name, but the folder holds no application.
        let other = root.join("Baska");
        std::fs::create_dir_all(&other).unwrap();
        assert_eq!(uninstall_dir(&other.join("uninstall.exe"), &p), None);
        // Application present, but not in our folder name.
        std::fs::write(other.join(&p.exe), b"x").unwrap();
        assert_eq!(uninstall_dir(&other.join("uninstall.exe"), &p), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}

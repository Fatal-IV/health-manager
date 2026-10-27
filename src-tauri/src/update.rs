//! Update check and install, fed by the GitHub releases of the project.
//!
//! Security notes: the UI never supplies a URL (the install command looks the
//! release up again); only `https://github.com/<REPO>/releases/download/...`
//! asset links are accepted; the installer is run only after its minisign
//! signature verifies against the public key built into this program, so a
//! tampered or swapped download is refused. Everything downloaded is size-capped.

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::io::Read;

/// `owner/name` of the GitHub repository that publishes the releases.
pub const REPO: &str = "Fatal-IV/health-manager";
const INSTALLER_ASSET: &str = "Saglik-Denetimi-Setup.exe";
const SIGNATURE_ASSET: &str = "Saglik-Denetimi-Setup.exe.sig";
/// minisign public key (the `.pub` file written by `tauri signer generate`).
const PUBLIC_KEY: &str = include_str!("../updater.pub");

const MAX_API_BYTES: u64 = 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 4096;
const MAX_INSTALLER_BYTES: u64 = 100 * 1024 * 1024;

/// What the UI may know about an available update. The download links stay in Rust.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    #[serde(skip)]
    installer_url: String,
    #[serde(skip)]
    signature_url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// `v1.2.3` or `1.2.3`, exactly three numeric parts.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.trim().trim_start_matches('v').split('.');
    let v = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    it.next().is_none().then_some(v)
}

fn allowed_url(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/releases/download/"))
}

/// The update described by a `releases/latest` answer, if it is newer than `current`
/// and carries the installer plus its signature.
pub fn parse_release(json: &str, current: &str) -> Option<UpdateInfo> {
    let r: Release = serde_json::from_str(json).ok()?;
    if r.draft || r.prerelease {
        return None;
    }
    let newest = parse_version(&r.tag_name)?;
    if newest <= parse_version(current)? {
        return None;
    }
    let url_of = |name: &str| {
        r.assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
            .filter(|u| allowed_url(u))
    };
    Some(UpdateInfo {
        version: format!("{}.{}.{}", newest.0, newest.1, newest.2),
        notes: r.body.unwrap_or_default().chars().take(2000).collect(),
        installer_url: url_of(INSTALLER_ASSET)?,
        signature_url: url_of(SIGNATURE_ASSET)?,
    })
}

/// Checks `data` against the base64 `.sig` text published next to the installer.
pub fn verify_signature(data: &[u8], signature_b64: &str) -> Result<(), String> {
    let key = PublicKey::decode(PUBLIC_KEY).map_err(|e| format!("public key: {e}"))?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| "signature is not base64".to_string())?;
    let text = String::from_utf8(raw).map_err(|_| "signature is not text".to_string())?;
    let sig = Signature::decode(&text).map_err(|e| format!("signature: {e}"))?;
    key.verify(data, &sig, true)
        .map_err(|_| "signature does not match".to_string())
}

fn get(url: &str, max: u64, timeout_secs: u64) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build();
    let resp = agent
        .get(url)
        .set("User-Agent", concat!("Saglik-Denetimi/", env!("CARGO_PKG_VERSION")))
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(max + 1)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > max {
        return Err("download too large".into());
    }
    Ok(buf)
}

/// Asks GitHub for the latest release. `Ok(None)` when this build is current (or nothing is published).
pub fn check() -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    match get(&url, MAX_API_BYTES, 15) {
        Ok(body) => Ok(parse_release(
            &String::from_utf8_lossy(&body),
            env!("CARGO_PKG_VERSION"),
        )),
        // A repository without releases answers 404: nothing newer exists.
        Err(e) if e.contains("404") => Ok(None),
        Err(e) => Err(e),
    }
}

/// Downloads the newest installer, verifies its signature and starts it.
/// The caller closes this program afterwards so the installer can replace the files.
pub fn download_and_launch() -> Result<(), String> {
    let info = check()?.ok_or_else(|| "no_update".to_string())?;
    let installer = get(&info.installer_url, MAX_INSTALLER_BYTES, 600)?;
    let signature = get(&info.signature_url, MAX_SIGNATURE_BYTES, 30)?;
    verify_signature(&installer, &String::from_utf8_lossy(&signature))?;

    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| e.to_string())?;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let path = std::env::temp_dir().join(format!("saglik-denetimi-update-{hex}.exe"));
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        f.write_all(&installer).map_err(|e| e.to_string())?;
    }
    launch_installer(&path)
}

/// The installer asks for administrator rights in its manifest, which `Command::spawn`
/// (CreateProcess) cannot satisfy: it fails with "elevation required". ShellExecuteExW
/// `runas` shows the Windows permission prompt instead; declining it is an error and the
/// caller keeps this program open.
#[cfg(windows)]
fn launch_installer(path: &std::path::Path) -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let file = HSTRING::from(path.as_os_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| format!("installer not started: {e}"))
}

#[cfg(not(windows))]
fn launch_installer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_update() -> Result<Option<UpdateInfo>, String> {
    tauri::async_runtime::spawn_blocking(check)
        .await
        .map_err(|e| e.to_string())?
}

/// Installs the newest release, then closes the app. Nothing is closed if anything fails.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(download_and_launch)
        .await
        .map_err(|e| e.to_string())??;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &[u8] = include_bytes!("../testdata/payload.txt");
    const PAYLOAD_SIG: &str = include_str!("../testdata/payload.txt.sig");

    fn release(tag: &str, extra: &str, installer: &str, sig: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","body":"notes",{extra}"assets":[
              {{"name":"Saglik-Denetimi-Setup.exe","browser_download_url":"{installer}"}},
              {{"name":"Saglik-Denetimi-Setup.exe.sig","browser_download_url":"{sig}"}}]}}"#
        )
    }

    fn good(tag: &str, extra: &str) -> String {
        let base = format!("https://github.com/{REPO}/releases/download/{tag}");
        release(
            tag,
            extra,
            &format!("{base}/Saglik-Denetimi-Setup.exe"),
            &format!("{base}/Saglik-Denetimi-Setup.exe.sig"),
        )
    }

    #[test]
    fn versions_parse_strictly_and_compare_numerically() {
        assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("1.2.x"), None);
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    }

    #[test]
    fn newer_release_is_offered() {
        let u = parse_release(&good("v0.3.0", ""), "0.2.4").unwrap();
        assert_eq!(u.version, "0.3.0");
        assert_eq!(u.notes, "notes");
    }

    #[test]
    fn same_older_draft_and_prerelease_are_not_offered() {
        assert!(parse_release(&good("v0.2.4", ""), "0.2.4").is_none());
        assert!(parse_release(&good("v0.2.0", ""), "0.2.4").is_none());
        assert!(parse_release(&good("v0.3.0", r#""draft":true,"#), "0.2.4").is_none());
        assert!(parse_release(&good("v0.3.0", r#""prerelease":true,"#), "0.2.4").is_none());
    }

    #[test]
    fn release_without_installer_or_signature_is_not_offered() {
        let only_installer = format!(
            r#"{{"tag_name":"v0.3.0","assets":[{{"name":"Saglik-Denetimi-Setup.exe","browser_download_url":"https://github.com/{REPO}/releases/download/v0.3.0/Saglik-Denetimi-Setup.exe"}}]}}"#
        );
        assert!(parse_release(&only_installer, "0.2.4").is_none());
        assert!(parse_release("{}", "0.2.4").is_none());
        assert!(parse_release("not json", "0.2.4").is_none());
    }

    #[test]
    fn links_outside_the_project_releases_are_refused() {
        let evil = release(
            "v0.3.0",
            "",
            "https://evil.example/Saglik-Denetimi-Setup.exe",
            "https://evil.example/Saglik-Denetimi-Setup.exe.sig",
        );
        assert!(parse_release(&evil, "0.2.4").is_none());
        let other_repo = release(
            "v0.3.0",
            "",
            "https://github.com/someone/else/releases/download/v0.3.0/Saglik-Denetimi-Setup.exe",
            "https://github.com/someone/else/releases/download/v0.3.0/Saglik-Denetimi-Setup.exe.sig",
        );
        assert!(parse_release(&other_repo, "0.2.4").is_none());
    }

    #[test]
    fn genuine_signature_verifies() {
        assert_eq!(verify_signature(PAYLOAD, PAYLOAD_SIG), Ok(()));
    }

    /// Release check: `RELEASE_DIR=dist/release/v<version> cargo test -p app -- --ignored release_files`
    #[test]
    #[ignore = "needs a built release folder"]
    fn release_files_verify_with_the_embedded_key() {
        let dir = std::path::PathBuf::from(std::env::var("RELEASE_DIR").expect("set RELEASE_DIR"));
        let setup = std::fs::read(dir.join("Saglik-Denetimi-Setup.exe")).unwrap();
        let sig = std::fs::read_to_string(dir.join("Saglik-Denetimi-Setup.exe.sig")).unwrap();
        assert_eq!(verify_signature(&setup, &sig), Ok(()));
    }

    #[test]
    fn changed_data_or_broken_signature_is_refused() {
        let mut tampered = PAYLOAD.to_vec();
        tampered[0] ^= 1;
        assert!(verify_signature(&tampered, PAYLOAD_SIG).is_err());
        assert!(verify_signature(PAYLOAD, "AAAA").is_err());
        assert!(verify_signature(PAYLOAD, "").is_err());
    }
}

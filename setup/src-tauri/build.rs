//! Stages the application files this installer carries. They are built first
//! (`scripts/release.mjs` does it in order); a plain dev build gets empty
//! placeholders and the installer then refuses to install.
use std::path::PathBuf;

fn stage(from: &PathBuf, to: &PathBuf) {
    println!("cargo:rerun-if-changed={}", from.display());
    match std::fs::copy(from, to) {
        Ok(_) => {}
        Err(_) => std::fs::write(to, []).expect("write placeholder"),
    }
}

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let release = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("..")
        .join("..")
        .join("target")
        .join("release");
    stage(&release.join("saglik-denetimi.exe"), &out.join("payload_app.exe"));
    stage(&release.join("fix-helper.exe"), &out.join("payload_helper.exe"));
    println!("cargo:rerun-if-changed=../setup.config.json");
    tauri_build::build()
}

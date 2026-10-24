// Builds the release fix-helper and copies it where Tauri expects a sidecar:
// src-tauri/binaries/fix-helper-<target-triple>.exe. Run by tauri.bundle.conf.json
// before a bundled build; not needed for development or `cargo test`.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, stdio: ["ignore", "pipe", "inherit"] }).toString();

const host = run("rustc", ["-vV"])
  .split(/\r?\n/)
  .find((l) => l.startsWith("host:"))
  ?.slice(5)
  .trim();
if (!host) throw new Error("could not read the rustc host triple");

execFileSync("cargo", ["build", "--release", "-p", "fix-helper"], { cwd: root, stdio: "inherit" });

const outDir = join(root, "src-tauri", "binaries");
mkdirSync(outDir, { recursive: true });
const dest = join(outDir, `fix-helper-${host}.exe`);
copyFileSync(join(root, "target", "release", "fix-helper.exe"), dest);
console.log(`sidecar ready: ${dest}`);

// Packs a no-install portable folder + zip: dist/Saglik-Denetimi/{Saglik Denetimi.exe, fix-helper.exe}.
// Run after `tauri build --no-bundle --config src-tauri/tauri.bundle.conf.json`.
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const rel = join(root, "target", "release");
const out = join(root, "dist", "Saglik-Denetimi");
rmSync(join(root, "dist"), { recursive: true, force: true });
mkdirSync(out, { recursive: true });
copyFileSync(join(rel, "app.exe"), join(out, "Saglik Denetimi.exe"));
copyFileSync(join(rel, "fix-helper.exe"), join(out, "fix-helper.exe"));
const zip = join(root, "dist", "Saglik-Denetimi.zip");
execFileSync("powershell", ["-NoProfile", "-Command", `Compress-Archive -Path '${out}' -DestinationPath '${zip}'`], { stdio: "inherit" });
console.log(`portable ready: ${zip}`);

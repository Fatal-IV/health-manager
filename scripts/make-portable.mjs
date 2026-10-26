// Packs a no-install portable folder: dist/Saglik-Denetimi-v<version>/{Saglik Denetimi.exe, fix-helper.exe}.
// Pass --zip to also make a zip for sharing (off by default).
// Run after `tauri build --no-bundle --config src-tauri/tauri.bundle.conf.json`.
import { copyFileSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const rel = join(root, "target", "release");
const version = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")).version;
const out = join(root, "dist", `Saglik-Denetimi-v${version}`);
rmSync(join(root, "dist"), { recursive: true, force: true });
mkdirSync(out, { recursive: true });
copyFileSync(join(rel, "app.exe"), join(out, "Saglik Denetimi.exe"));
copyFileSync(join(rel, "fix-helper.exe"), join(out, "fix-helper.exe"));
if (process.argv.includes("--zip")) {
  const zip = join(root, "dist", `Saglik-Denetimi-v${version}.zip`);
  execFileSync("powershell", ["-NoProfile", "-Command", `Compress-Archive -Path '${out}' -DestinationPath '${zip}'`], { stdio: "inherit" });
  console.log(`zip ready: ${zip}`);
}
console.log(`portable ready: ${out}`);

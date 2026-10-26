// Raises the app version everywhere it is kept: node scripts/bump-version.mjs [patch|minor|major]
// (default patch). Run it once per change that is handed to the user, so the version shown in the
// title bar and in the portable folder name tells which build is running.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const part = process.argv[2] ?? "patch";
if (!["patch", "minor", "major"].includes(part)) throw new Error(`unknown part: ${part}`);

const confPath = join(root, "src-tauri", "tauri.conf.json");
const [maj, min, pat] = JSON.parse(readFileSync(confPath, "utf8")).version.split(".").map(Number);
const next = part === "major" ? `${maj + 1}.0.0` : part === "minor" ? `${maj}.${min + 1}.0` : `${maj}.${min}.${pat + 1}`;

function rewrite(file, pattern) {
  const p = join(root, file);
  const text = readFileSync(p, "utf8");
  if (!pattern.test(text)) throw new Error(`no version found in ${file}`);
  writeFileSync(p, text.replace(pattern, (_, pre, post) => `${pre}${next}${post}`));
}

rewrite("src-tauri/tauri.conf.json", /("version":\s*")[^"]+(")/);
rewrite("ui/package.json", /("version":\s*")[^"]+(")/);
rewrite("setup/src-tauri/tauri.conf.json", /("version":\s*")[^"]+(")/);
for (const c of ["src-tauri", "setup/src-tauri", "crates/fix-helper", "crates/health-core"]) {
  rewrite(`${c}/Cargo.toml`, /^(version = ")[^"]+(")/m);
}
console.log(`version -> ${next}`);

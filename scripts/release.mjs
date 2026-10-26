// Builds the app and its installer, signs the installer for the in-app updater and (only with --publish) uploads it as a GitHub release.
//   node scripts/release.mjs            build + sign into dist/release/v<version>/, upload nothing
//   node scripts/release.mjs --publish  the same, then `gh release create` on the project repository
// The signing key lives outside the repository (default ~/.saglik-denetimi/updater.key; override with
// TAURI_SIGNING_PRIVATE_KEY_PATH, and TAURI_SIGNING_PRIVATE_KEY_PASSWORD if it has a password).
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = "Fatal-IV/saglik-denetimi"; // keep in step with REPO in src-tauri/src/update.rs
const publish = process.argv.includes("--publish");
const version = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")).version;
const keyPath = process.env.TAURI_SIGNING_PRIVATE_KEY_PATH ?? join(homedir(), ".saglik-denetimi", "updater.key");
if (!existsSync(keyPath)) throw new Error(`signing key not found: ${keyPath}`);

const tauriCli = join(root, "ui", "node_modules", "@tauri-apps", "cli", "tauri.js");
const env = { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PATH: keyPath, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "" };
const tauri = (...args) => {
  const opts = typeof args[args.length - 1] === "object" ? args.pop() : {};
  execFileSync(process.execPath, [tauriCli, ...args], { cwd: root, env, stdio: "inherit", ...opts });
};

// 1) The application and its helper, 2) the installer that carries both (its build.rs embeds them).
tauri("build", "--no-bundle", "--config", "src-tauri/tauri.bundle.conf.json");
tauri("build", "--no-bundle", { cwd: join(root, "setup") });
const built = join(root, "target", "release", "saglik-denetimi-setup.exe");
if (!existsSync(built)) throw new Error(`installer not built: ${built}`);

// ASCII names: they are the asset names the updater looks for (src-tauri/src/update.rs).
const out = join(root, "dist", "release", `v${version}`);
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const setup = join(out, "Saglik-Denetimi-Setup.exe");
copyFileSync(built, setup);
tauri("signer", "sign", setup);
const hash = createHash("sha256").update(readFileSync(setup)).digest("hex");
writeFileSync(`${setup}.sha256`, `${hash}  Saglik-Denetimi-Setup.exe\n`);
const notes = join(out, "notes.md");
writeFileSync(notes, `Sağlık Denetimi ${version}\n\nSHA-256: \`${hash}\`\n`);
console.log(`release ready: ${out}\nSHA-256 ${hash}`);

if (publish) {
  const assets = [setup, `${setup}.sig`, `${setup}.sha256`];
  execFileSync("gh", ["release", "create", `v${version}`, ...assets, "--repo", repo, "--title", `Sağlık Denetimi ${version}`, "--notes-file", notes], { cwd: root, stdio: "inherit" });
  console.log(`published: https://github.com/${repo}/releases/tag/v${version}`);
} else {
  console.log("not published (use --publish after the repository exists)");
}

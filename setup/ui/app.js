// Fills the window from the data the program hands over (setup.config.json + current state).
// Colours and every text come from the config; this file only decides which screen is shown.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const appWindow = window.__TAURI__.window.getCurrentWindow();

const $ = (id) => document.getElementById(id);
let info = null;
let busy = false;

const fill = (text, extra = {}) => {
  const vars = { name: info.config.product.name, tagline: info.config.texts.tagline, version: info.version, installed: info.installed ?? "", ...extra };
  return String(text ?? "").replace(/\{\{(\w+)\}\}/g, (_, k) => vars[k] ?? "");
};
const show = (id) => {
  for (const p of ["ready", "working", "done", "error"]) $(p).hidden = p !== id;
};
const setBusy = (b) => {
  busy = b;
  $("close").disabled = b;
};

function applyTheme(theme) {
  for (const [k, v] of Object.entries(theme ?? {})) document.documentElement.style.setProperty(`--${k}`, v);
}

function render() {
  const tx = info.config.texts;
  const m = tx[info.mode];
  document.title = fill(m.title);
  $("ready-title").textContent = fill(m.title);
  $("ready-sub").textContent = fill(m.sub);
  $("go").textContent = fill(m.button);
  $("ready-terms").textContent = fill(m.terms);
  $("desktop-label").textContent = tx.desktop_shortcut;
  $("desktop-row").hidden = info.mode === "uninstall";
  $("admin-note").textContent = tx.admin_note ?? "";
  $("working-title").textContent = fill(m.title);
  $("status").textContent = fill(m.working);
  $("close").setAttribute("aria-label", tx.close);
  $("error-title").textContent = tx.errors.title;
  $("error-close").textContent = tx.close;
  $("retry").textContent = tx.errors.retry;
  $("finish").textContent = fill(m.done_button);
  if (!info.payload_ok && info.mode !== "uninstall") {
    $("go").disabled = true;
    $("ready-terms").textContent = tx.no_payload;
  }
}

function fail(code, detail) {
  const tx = info.config.texts;
  $("error-text").textContent = fill(tx.errors[code] ?? tx.errors.generic);
  $("error-detail").textContent = detail ? `${tx.log_prefix}: ${detail}` : "";
  setBusy(false);
  show("error");
}

async function run() {
  if (busy) return;
  setBusy(true);
  const m = info.config.texts[info.mode];
  $("fill").style.width = "0%";
  $("status").textContent = fill(m.working);
  show("working");
  try {
    await invoke("run_setup", { options: { desktop_shortcut: $("desktop").checked } });
    $("done-title").textContent = fill(m.done_title);
    $("done-text").textContent = fill(m.done_text);
    setBusy(false);
    // The program's own updater asked for a restart: skip the done screen.
    if (info.relaunch) return void finish();
    show("done");
  } catch (e) {
    const err = typeof e === "object" && e ? e : { code: "generic", detail: String(e) };
    fail(err.code ?? "generic", err.detail);
  }
}

async function finish() {
  if (info.mode !== "uninstall") await invoke("launch_app").catch(() => {});
  await appWindow.close();
}

async function start() {
  info = await invoke("get_setup_info");
  applyTheme(info.config.theme);
  render();
  await listen("setup-progress", (ev) => {
    $("fill").style.width = `${ev.payload.percent}%`;
    $("status").textContent = ev.payload.message;
    document.querySelector(".bar").setAttribute("aria-valuenow", String(ev.payload.percent));
  });
  $("go").addEventListener("click", run);
  $("retry").addEventListener("click", run);
  $("finish").addEventListener("click", finish);
  $("close").addEventListener("click", () => !busy && appWindow.close());
  $("error-close").addEventListener("click", () => appWindow.close());
  if (info.auto_start && info.payload_ok) void run();
  else show("ready");
}

void start();

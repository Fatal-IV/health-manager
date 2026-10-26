import { CopySimple, Minus, Square, X } from "@phosphor-icons/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { t } from "../i18n";

// Outside Tauri (tests, plain browser) there is no window handle; controls become no-ops.
function appWindow() {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = appWindow();
    if (!win) return;
    let off: (() => void) | undefined;
    let alive = true;
    const sync = () => void win.isMaximized().then((m) => alive && setMaximized(m)).catch(() => {});
    sync();
    win.onResized(sync).then((u) => (alive ? (off = u) : u())).catch(() => {});
    return () => {
      alive = false;
      off?.();
    };
  }, []);

  const run = (fn: (w: NonNullable<ReturnType<typeof appWindow>>) => Promise<unknown>) => () => {
    const w = appWindow();
    if (w) void fn(w).catch(() => {});
  };

  return (
    <header className="titlebar" data-tauri-drag-region onDoubleClick={run((w) => w.toggleMaximize())}>
      <div className="titlebar-brand" data-tauri-drag-region>
        <svg width="22" height="22" viewBox="0 0 512 512" fill="none" aria-hidden="true">
          <path d="M256 36C196 78 126 92 72 92v158c0 108 76 188 184 228 108-40 184-120 184-228V92c-54 0-124-14-184-56Z" fill="#ff4a1c" />
          <path d="M104 262h78l34-82 56 164 40-110 20 28h76" stroke="#fff" strokeWidth="28" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span data-tauri-drag-region>{t("app.title")}</span>
        <span className="titlebar-version" data-tauri-drag-region>
          v{__APP_VERSION__}
        </span>
      </div>
      <div className="titlebar-controls" onDoubleClick={(e) => e.stopPropagation()}>
        <button type="button" className="wc" aria-label="Küçült" onClick={run((w) => w.minimize())}>
          <Minus size={14} />
        </button>
        <button type="button" className="wc" aria-label={maximized ? "Önceki boyut" : "Büyüt"} onClick={run((w) => w.toggleMaximize())}>
          {maximized ? <CopySimple size={13} /> : <Square size={12} />}
        </button>
        <button type="button" className="wc wc-close" aria-label="Kapat" onClick={run((w) => w.close())}>
          <X size={14} />
        </button>
      </div>
    </header>
  );
}

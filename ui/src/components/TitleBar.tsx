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
        <svg width="20" height="20" viewBox="0 0 20 20" fill="none" aria-hidden="true">
          <rect width="20" height="20" rx="6" fill="#ff4a1c" />
          <path d="M3 10h3l1.6-4 2.6 8 1.8-4H17" stroke="#fff" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
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

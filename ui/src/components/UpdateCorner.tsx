import { ArrowsClockwise, CheckCircle, Sparkle, WarningCircle, X } from "@phosphor-icons/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, type ReleaseNotes, type UpdateInfo } from "../api";
import { t } from "../i18n";

const SEEN_KEY = "saglik-denetimi.seen-version";

type Phase = "idle" | "checking" | "current" | "installing" | "failed" | "news";

const readSeen = (): string | null => {
  try {
    return localStorage.getItem(SEEN_KEY);
  } catch {
    return null;
  }
};
const writeSeen = (v: string) => {
  try {
    localStorage.setItem(SEEN_KEY, v);
  } catch {
    /* a browser without storage just shows the news again next time */
  }
};

/** One list item per `- ` line of a release's notes. */
export function noteLines(notes: string): string[] {
  return notes
    .split(/\r?\n/)
    .map((l) => l.replace(/^\s*[-*]\s+/, "").trim())
    .filter(Boolean);
}

/**
 * The corner box at the bottom right. Clicking it grows it into the window and checks for a newer
 * release at once; a newer one is downloaded and installed (the program restarts by itself).
 * After such a restart the same box opens by itself and lists what the new version brings.
 */
export function UpdateCorner() {
  const [open, setOpen] = useState(false);
  const [phase, setPhase] = useState<Phase>("idle");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [news, setNews] = useState<ReleaseNotes | null>(null);
  const busy = useRef(false);

  // First start after an update: what the new version brings. Never on the very first start, and
  // an unreachable network keeps the news for the next start.
  useEffect(() => {
    let alive = true;
    void api.getReleaseNotes().then((r) => {
      if (!alive || !r.ok) return;
      const seen = readSeen();
      if (seen === __APP_VERSION__) return;
      if (seen && r.value) {
        setNews(r.value);
        setPhase("news");
        setOpen(true);
      } else {
        writeSeen(__APP_VERSION__);
      }
    });
    return () => {
      alive = false;
    };
  }, []);

  const run = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setPhase("checking");
    const found = await api.checkUpdate();
    if (!found.ok) {
      setPhase("failed");
      busy.current = false;
      return;
    }
    if (!found.value) {
      setPhase("current");
      busy.current = false;
      return;
    }
    setUpdate(found.value);
    setPhase("installing");
    // On success the program closes itself and the installer starts it again.
    const done = await api.installUpdate();
    if (!done.ok) setPhase("failed");
    busy.current = false;
  }, []);

  const openBox = () => {
    setOpen(true);
    if (phase === "idle" || phase === "current" || phase === "failed") void run();
  };
  const closeBox = () => {
    if (busy.current) return;
    setOpen(false);
    if (phase === "news") {
      writeSeen(__APP_VERSION__);
      setPhase("idle");
    } else if (phase !== "idle") {
      setPhase("idle");
    }
  };

  const working = phase === "checking" || phase === "installing";
  const shown = update?.version ?? __APP_VERSION__;

  return (
    <>
      {!open && (
        <button type="button" className="upd-pill" onClick={openBox} aria-label={t("ui.update.corner")}>
          <ArrowsClockwise size={18} aria-hidden />
          <span>{t("ui.update.corner")}</span>
        </button>
      )}
      <section className="upd-panel" data-open={open} aria-hidden={!open} role="status" aria-live="polite">
        <header>
          <span className="upd-icon" aria-hidden>
            {phase === "failed" ? <WarningCircle size={22} /> : phase === "news" ? <Sparkle size={22} weight="fill" /> : phase === "current" ? <CheckCircle size={22} weight="fill" /> : <ArrowsClockwise size={22} className={working ? "spin" : undefined} />}
          </span>
          <h2>
            {phase === "news" && news
              ? t("ui.update.news_title", { version: news.version })
              : phase === "failed"
                ? t("ui.update.failed_title")
                : phase === "current"
                  ? t("ui.update.current_title")
                  : phase === "installing"
                    ? t("ui.update.installing_title", { version: shown })
                    : t("ui.update.checking_title")}
          </h2>
          <button type="button" className="upd-close" onClick={closeBox} disabled={working} aria-label={t("ui.update.close")} tabIndex={open ? 0 : -1}>
            <X size={16} aria-hidden />
          </button>
        </header>

        {working && <div className="upd-bar" aria-hidden><span /></div>}
        {phase === "checking" && <p>{t("ui.update.checking")}</p>}
        {phase === "installing" && <p>{t("ui.update.installing", { version: shown })}</p>}
        {phase === "current" && <p>{t("ui.update.current", { version: __APP_VERSION__ })}</p>}
        {phase === "failed" && (
          <>
            <p>{t("ui.update.failed")}</p>
            <button type="button" className="primary small" onClick={() => void run()} tabIndex={open ? 0 : -1}>
              {t("ui.update.retry")}
            </button>
          </>
        )}
        {phase === "news" && news && (
          <>
            <ul>
              {noteLines(news.notes).map((l) => (
                <li key={l}>{l}</li>
              ))}
            </ul>
            <button type="button" className="primary small" onClick={closeBox} tabIndex={open ? 0 : -1}>
              {t("ui.update.news_ok")}
            </button>
          </>
        )}
      </section>
    </>
  );
}

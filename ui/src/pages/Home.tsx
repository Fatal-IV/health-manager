import { useState } from "react";
import type { ScanMode } from "../api";
import { ScoreRing } from "../components/ScoreRing";
import { t } from "../i18n";

const WELCOME_KEY = "saglik-denetimi.welcomed";

function alreadyWelcomed(): boolean {
  try {
    return localStorage.getItem(WELCOME_KEY) === "1";
  } catch {
    return false; // storage can be unavailable; showing the welcome again is harmless
  }
}

function markWelcomed() {
  try {
    localStorage.setItem(WELCOME_KEY, "1");
  } catch {
    /* ignore */
  }
}

export function Home({
  onStart,
  lastScan,
  score,
}: {
  onStart: (mode: ScanMode) => void;
  lastScan?: string;
  score?: number | null;
}) {
  const [welcome, setWelcome] = useState(() => !alreadyWelcomed());
  const [deep, setDeep] = useState(false);
  return (
    <main className="center">
      <h1>{t("ui.home.title")}</h1>
      <p className="muted">{t("ui.home.subtitle")}</p>

      {welcome && (
        <section className="card" role="region" aria-label={t("ui.home.welcome_title")}>
          <h2>{t("ui.home.welcome_title")}</h2>
          <p>{t("ui.home.welcome_body")}</p>
          <button
            type="button"
            onClick={() => {
              markWelcomed();
              setWelcome(false);
            }}
          >
            {t("ui.home.welcome_ok")}
          </button>
        </section>
      )}

      <ScoreRing value={score ?? null} />
      <p className="muted">
        {lastScan ? t("ui.home.last_scan", { date: new Date(lastScan).toLocaleString("tr-TR") }) : t("ui.home.never")}
      </p>

      <p>
        <button type="button" className="primary" onClick={() => onStart(deep ? "deep" : "quick")}>
          {t("ui.home.start")}
        </button>
      </p>
      <label className="muted">
        <input type="checkbox" checked={deep} onChange={(e) => setDeep(e.target.checked)} /> {t("ui.home.deep")}
      </label>
    </main>
  );
}

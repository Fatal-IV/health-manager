import { useEffect, useState } from "react";
import { api, type TroubleshooterId, type TroubleshootState } from "../api";
import { t } from "../i18n";
import { TROUBLESHOOTERS } from "./TroubleshootPicker";

type Row = "loading" | "healthy" | "problem" | "unmeasured" | "notest";

/** What a troubleshooter's first answer means for the overview. A test that could not run is never "healthy". */
function rowOf(r: Awaited<ReturnType<typeof api.troubleshootStart>>): Row {
  if (!r.ok) return "unmeasured";
  const s: TroubleshootState = r.value;
  if (s.state === "healthy") return "healthy";
  if (!s.tested) return "notest";
  return s.unmeasured ? "unmeasured" : "problem";
}

/**
 * The combined result: the scan's score and what each troubleshooter's test found.
 * Only tests run here; nothing is repaired until the user opens a troubleshooter.
 */
export function Overview({
  score,
  onOpen,
  onResults,
  onBack,
}: {
  score: number | null;
  onOpen: (id: TroubleshooterId) => void;
  onResults: () => void;
  onBack: () => void;
}) {
  const [rows, setRows] = useState<Record<string, Row>>({});

  useEffect(() => {
    let alive = true;
    void Promise.all(
      TROUBLESHOOTERS.map(async (id) => {
        const row = rowOf(await api.troubleshootStart(id));
        if (alive) setRows((r) => ({ ...r, [id]: row }));
      }),
    );
    return () => {
      alive = false;
    };
  }, []);

  return (
    <main>
      <h1>{t("ui.overview.title")}</h1>
      <p className="muted">{score === null ? t("ui.overview.no_score") : t("ui.overview.score_line", { score })}</p>
      <ul className="plain">
        {TROUBLESHOOTERS.map((id) => {
          const row = rows[id] ?? "loading";
          return (
            <li key={id} data-testid={`overview-${id}`} className="card">
              <strong>{t(`ui.troubleshoot.titles.${id}`)}</strong>
              <span role={row === "loading" ? "status" : undefined}>{t(`ui.overview.status_${row}`)}</span>
              {(row === "problem" || row === "unmeasured" || row === "notest") && (
                <button type="button" className={row === "problem" ? "primary" : undefined} onClick={() => onOpen(id)}>
                  {row === "problem" ? t("ui.overview.fix") : t("ui.overview.open")}
                </button>
              )}
            </li>
          );
        })}
      </ul>
      <div className="row">
        <button type="button" className="primary" onClick={onResults}>
          {t("ui.overview.see_results")}
        </button>
        <button type="button" onClick={onBack}>
          {t("ui.troubleshoot.back")}
        </button>
      </div>
    </main>
  );
}

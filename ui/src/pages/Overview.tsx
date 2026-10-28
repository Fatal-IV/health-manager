import { ArrowLeft, ListChecks } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { api, TROUBLESHOOTERS, type Status, type TroubleshooterId, type TroubleshootState } from "../api";
import { ActionDock, type DockAction } from "../components/ActionDock";
import { ScoreRing } from "../components/ScoreRing";
import { StatusIcon } from "../components/StatusIcon";
import { t } from "../i18n";

type Row = "loading" | "healthy" | "problem" | "unmeasured" | "notest";

/** What a troubleshooter's first answer means for the overview. A test that could not run is never "healthy". */
function rowOf(r: Awaited<ReturnType<typeof api.troubleshootStart>>): Row {
  if (!r.ok) return "unmeasured";
  const s: TroubleshootState = r.value;
  if (s.state === "healthy") return "healthy";
  if (!s.tested) return "notest";
  return s.unmeasured ? "unmeasured" : "problem";
}

const ICON: Record<Row, Status> = {
  loading: "unknown",
  healthy: "good",
  problem: "warning",
  unmeasured: "unknown",
  notest: "unknown",
};

/** One sentence for the whole screen. "Everything is fine" is only said when every test passed. */
function headline(rows: Row[]): string {
  if (rows.includes("loading")) return t("ui.overview.summary_loading");
  const problems = rows.filter((r) => r === "problem").length;
  if (problems > 0) return t("ui.overview.summary_problems", { count: problems });
  const unchecked = rows.filter((r) => r !== "healthy").length;
  if (unchecked > 0) return t("ui.overview.summary_unchecked", { count: unchecked });
  return t("ui.overview.summary_ok");
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

  const rowFor = (id: TroubleshooterId): Row => rows[id] ?? "loading";
  const actions: DockAction[] = [
    { key: "results", label: t("ui.overview.see_results"), icon: ListChecks, onClick: onResults, primary: true },
    { key: "back", label: t("ui.troubleshoot.back"), icon: ArrowLeft, onClick: onBack },
  ];

  return (
    <main className="overview">
      <p className="eyebrow">{t("ui.home.eyebrow")}</p>
      <h1>{t("ui.overview.title")}</h1>

      <section className="ov-summary">
        <ScoreRing value={score} size={76} />
        <div>
          <h2>{headline(TROUBLESHOOTERS.map(rowFor))}</h2>
          <p className="muted">{score === null ? t("ui.overview.no_score") : t("ui.overview.score_line", { score })}</p>
          <p className="muted">{t("ui.home.trust")}</p>
        </div>
      </section>

      <ul className="ov-list" aria-label={t("ui.overview.list_label")}>
        {TROUBLESHOOTERS.map((id) => {
          const row = rowFor(id);
          const open = row === "problem" || row === "unmeasured" || row === "notest";
          return (
            <li key={id} data-testid={`overview-${id}`} data-state={row} className="ov-item">
              <span className="ov-icon">
                <StatusIcon status={ICON[row]} />
              </span>
              <div className="ov-text">
                <strong>{t(`ui.troubleshoot.titles.${id}`)}</strong>
                <span className="muted">{t(`ui.troubleshoot.descriptions.${id}`)}</span>
              </div>
              <span className="ov-pill" role={row === "loading" ? "status" : undefined}>
                {t(`ui.overview.status_${row}`)}
              </span>
              {open && (
                <button type="button" className={row === "problem" ? "primary" : undefined} onClick={() => onOpen(id)}>
                  {row === "problem" ? t("ui.overview.fix") : t("ui.overview.open")}
                </button>
              )}
            </li>
          );
        })}
      </ul>

      <ActionDock label={t("ui.home.actions")} actions={actions} />
    </main>
  );
}

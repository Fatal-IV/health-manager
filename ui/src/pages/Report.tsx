import { save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { api, type AppliedFix, type ReportFormat, type ScanReport } from "../api";
import { t } from "../i18n";

export function Report({
  report,
  applied,
  onBack,
}: {
  report: ScanReport;
  applied: AppliedFix[];
  onBack: () => void;
}) {
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const issues = report.results.flatMap((r) => r.issues);
  const titleOf = new Map(issues.map((i) => [i.id, i]));

  const exportAs = async (format: ReportFormat) => {
    const ext = format === "pdf" ? "pdf" : "txt";
    let path: string | null;
    try {
      path = await save({ defaultPath: `saglik-raporu.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
    } catch {
      path = null;
    }
    if (!path) return; // the user closed the dialog: nothing to do
    const r = await api.exportReport(format, path);
    setMessage(r.ok ? { ok: true, text: t("ui.report.saved") } : { ok: false, text: t("ui.report.save_failed") });
  };

  const penalties = report.score.breakdown.filter((b) => b.penalty > 0);
  return (
    <main>
      <h1>{t("ui.report.title")}</h1>
      <p className="muted">
        {report.cancelled ? t("ui.results.incomplete") : `${t("ui.results.score")}: ${report.score.value}/100`}
      </p>

      <section className="card">
        <h2>{t("ui.report.applied")}</h2>
        {applied.length === 0 ? (
          <p className="muted">{t("ui.report.none")}</p>
        ) : (
          <ul className="plain">
            {applied.map((a, n) => (
              <li key={`${a.fix}-${n}`}>
                {t(`fix.${a.fix}.summary`)} — {t(`report.outcome.${a.outcome}`)}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="card">
        <h2>{t("ui.report.remaining")}</h2>
        {issues.length === 0 ? (
          <p className="muted">{t("ui.report.none")}</p>
        ) : (
          <ul className="plain">
            {issues.map((i) => (
              <li key={i.id}>
                <span className={`badge s-${i.status}`}>{t(`status.${i.status}`)}</span> {t(i.title_key, i.params)}
              </li>
            ))}
          </ul>
        )}
      </section>

      {!report.cancelled && (
        <details className="card">
          <summary>{t("ui.results.why_score")}</summary>
          {penalties.length === 0 ? (
            <p>{t("ui.report.no_penalties")}</p>
          ) : (
            <ul className="plain">
              {penalties.map((b) => {
                const i = titleOf.get(b.issue_id);
                return (
                  <li key={b.issue_id}>
                    {i ? t(i.title_key, i.params) : b.issue_id} — {t("ui.report.penalty", { points: b.penalty })}
                  </li>
                );
              })}
            </ul>
          )}
        </details>
      )}

      <div className="row">
        <button type="button" className="primary" onClick={() => void exportAs("text")}>
          {t("ui.report.save_text")}
        </button>
        <button type="button" className="primary" onClick={() => void exportAs("pdf")}>
          {t("ui.report.save_pdf")}
        </button>
        <button type="button" onClick={onBack}>
          {t("ui.report.back")}
        </button>
      </div>
      {message && (
        <p role="status" className={message.ok ? "s-good" : "err"}>
          {message.text}
        </p>
      )}
    </main>
  );
}

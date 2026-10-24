import type { Category, ScanReport } from "../api";
import { CategoryCard } from "../components/CategoryCard";
import type { FixHandler } from "../components/IssueItem";
import { ScoreRing } from "../components/ScoreRing";
import { t } from "../i18n";

const ORDER: Category[] = ["security", "update", "storage", "performance", "system"];

export function Results({
  report,
  onFix,
  onReport,
  onRescan,
}: {
  report: ScanReport;
  onFix: FixHandler;
  onReport: () => void;
  onRescan: () => void;
}) {
  return (
    <main>
      <h1 className="center">{t("ui.results.title")}</h1>
      <ScoreRing value={report.cancelled ? null : report.score.value} />
      {report.cancelled && (
        <p className="center muted" role="status">
          {t("ui.results.incomplete")}
        </p>
      )}
      {ORDER.map((c) => {
        const results = report.results.filter((r) => r.category === c);
        return results.length > 0 ? <CategoryCard key={c} category={c} results={results} onFix={onFix} /> : null;
      })}
      <div className="row" style={{ marginTop: 20 }}>
        <button type="button" className="primary" onClick={onReport}>
          {t("ui.results.see_report")}
        </button>
        <button type="button" onClick={onRescan}>
          {t("ui.results.rescan")}
        </button>
      </div>
    </main>
  );
}

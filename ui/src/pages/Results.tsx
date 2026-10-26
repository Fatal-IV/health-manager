import { ArrowClockwise, FileText } from "@phosphor-icons/react";
import { useState } from "react";
import type { Category, ScanReport } from "../api";
import { CategoryPanel, RANK, categoryStatus } from "../components/CategoryCard";
import { Dock } from "../components/Dock";
import type { FixHandler } from "../components/IssueItem";
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
  const items = ORDER.map((category) => ({ category, results: report.results.filter((r) => r.category === category) })).filter(
    (i) => i.results.length > 0,
  );
  const [chosen, setChosen] = useState<Category | null>(null);
  // Open on the area that needs attention first, so the first thing seen is the thing to do.
  const worst = items.reduce<(typeof items)[number] | undefined>(
    (w, i) => (!w || RANK[categoryStatus(i.results)] > RANK[categoryStatus(w.results)] ? i : w),
    undefined,
  );
  const current = items.find((i) => i.category === chosen) ?? worst;
  const score = report.cancelled ? null : report.score.value;
  const shown = score === null ? "—" : String(score);
  const total = items.reduce((n, i) => n + i.results.reduce((m, r) => m + r.issues.length, 0), 0);

  return (
    <div className="stage">
      <div className="stage-score">
        <div
          className={`bignum${shown.length > 2 ? " wide" : ""}`}
          role="img"
          aria-label={`${t("ui.results.score")}: ${shown}`}
        >
          {shown}
        </div>
        <p className="bignum-sub" aria-hidden="true">
          /100 {t("ui.results.score").toLocaleLowerCase("tr-TR")}
        </p>
        <h1>{t("ui.results.title")}</h1>
        <p className="muted">{report.cancelled ? <span role="status">{t("ui.results.incomplete")}</span> : t("ui.results.issues_count", { count: total })}</p>
        <div className="actions">
          <button type="button" className="primary" onClick={onReport}>
            <FileText size={17} aria-hidden />
            {t("ui.results.see_report")}
          </button>
          <button type="button" onClick={onRescan}>
            <ArrowClockwise size={17} aria-hidden />
            {t("ui.results.rescan")}
          </button>
        </div>
      </div>
      {current && <CategoryPanel key={current.category} category={current.category} results={current.results} onFix={onFix} />}
      {current && <Dock items={items} selected={current.category} onSelect={setChosen} />}
    </div>
  );
}

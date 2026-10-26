import { ArrowClockwise, FileText } from "@phosphor-icons/react";
import { useState } from "react";
import type { Category, Issue, ScanReport } from "../api";
import { CategoryPanel, RANK, categoryStatus } from "../components/CategoryCard";
import { Dock } from "../components/Dock";
import { canBatch, type FixHandler } from "../components/IssueItem";
import { t } from "../i18n";

const ORDER: Category[] = ["security", "update", "storage", "performance", "system"];

export function Results({
  report,
  onFix,
  onReport,
  onRescan,
  onRescanArea,
  onBulkFix,
  initial,
}: {
  report: ScanReport;
  onFix: FixHandler;
  onReport: () => void;
  onRescan: () => void;
  onRescanArea?: (c: Category) => void;
  onBulkFix?: (issues: Issue[]) => void;
  initial?: Category;
}) {
  const items = ORDER.map((category) => ({ category, results: report.results.filter((r) => r.category === category) })).filter(
    (i) => i.results.length > 0,
  );
  const [chosen, setChosen] = useState<Category | null>(initial ?? null);
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set());
  // Open on the area that needs attention first, so the first thing seen is the thing to do.
  const worst = items.reduce<(typeof items)[number] | undefined>(
    (w, i) => (!w || RANK[categoryStatus(i.results)] > RANK[categoryStatus(w.results)] ? i : w),
    undefined,
  );
  const current = items.find((i) => i.category === chosen) ?? worst;
  const score = report.cancelled ? null : report.score.value;
  const shown = score === null ? "—" : String(score);
  // Only issues that still exist and can be fixed stay picked after a re-scan.
  const fixable = items.flatMap((i) => i.results.flatMap((r) => r.issues)).filter(canBatch);
  const chosenIssues = fixable.filter((i) => picked.has(i.id));
  const pick = (issue: Issue, on: boolean) =>
    setPicked((p) => {
      const n = new Set(p);
      if (on) n.add(issue.id);
      else n.delete(issue.id);
      return n;
    });
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
      {current && (
        <div className="pane-wrap">
          <CategoryPanel
            key={current.category}
            category={current.category}
            results={current.results}
            onFix={onFix}
            picked={picked}
            onPick={onBulkFix ? pick : undefined}
            onRescanArea={onRescanArea ? () => onRescanArea(current.category) : undefined}
          />
          {onBulkFix && chosenIssues.length > 0 && (
            <button type="button" className="primary bulk" onClick={() => onBulkFix(chosenIssues)}>
              {t("ui.results.fix_selected")} ({chosenIssues.length})
            </button>
          )}
        </div>
      )}
      {current && <Dock items={items} selected={current.category} onSelect={setChosen} />}
    </div>
  );
}

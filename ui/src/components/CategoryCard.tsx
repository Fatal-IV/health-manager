import { useState } from "react";
import type { Category, CheckResult, Status } from "../api";
import { t } from "../i18n";
import { IssueItem, type FixHandler } from "./IssueItem";

const RANK: Record<Status, number> = { good: 0, unknown: 1, warning: 2, critical: 3 };

/** Worst status of a category: a check that could not run counts as unknown, never as good. */
export function categoryStatus(results: CheckResult[]): Status {
  let worst: Status = "good";
  for (const r of results) {
    if (r.error !== null && RANK.unknown > RANK[worst]) worst = "unknown";
    for (const i of r.issues) if (RANK[i.status] > RANK[worst]) worst = i.status;
  }
  return worst;
}

export function CategoryCard({
  category,
  results,
  onFix,
}: {
  category: Category;
  results: CheckResult[];
  onFix: FixHandler;
}) {
  const status = categoryStatus(results);
  const issues = results.flatMap((r) => r.issues);
  const failed = results.filter((r) => r.error !== null);
  const [open, setOpen] = useState(status === "critical" || status === "warning");
  const panelId = `panel-${category}`;
  return (
    <section className="card" data-testid={`card-${category}`} data-status={status}>
      <button
        type="button"
        className="row between"
        style={{ width: "100%", background: "transparent", border: 0, padding: 0 }}
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen(!open)}
      >
        <strong>{t(`category.${category}`)}</strong>
        <span className={`badge s-${status}`}>
          {issues.length > 0 ? t("ui.results.issues_count", { count: issues.length }) : t(`status.${status}`)}
        </span>
      </button>
      {open && (
        <div id={panelId}>
          {issues.map((i) => (
            <IssueItem key={i.id} issue={i} onFix={onFix} />
          ))}
          {failed.length > 0 && <p className="muted">{t("ui.results.could_not_check")}</p>}
          {issues.length === 0 && failed.length === 0 && <p className="muted">{t("ui.results.all_good")}</p>}
        </div>
      )}
    </section>
  );
}

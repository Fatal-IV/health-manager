import { ArrowsClockwise, Desktop, Gauge, HardDrives, ShieldCheck, type Icon as PhosphorIcon } from "@phosphor-icons/react";
import type { Category, CheckResult, Status } from "../api";
import { t } from "../i18n";
import { IssueItem, type FixHandler } from "./IssueItem";

export const CATEGORY_ICONS: Record<Category, PhosphorIcon> = {
  security: ShieldCheck,
  update: ArrowsClockwise,
  storage: HardDrives,
  performance: Gauge,
  system: Desktop,
};

export const RANK: Record<Status, number> = { good: 0, unknown: 1, warning: 2, critical: 3 };

/** Worst status of a category: a check that could not run counts as unknown, never as good. */
export function categoryStatus(results: CheckResult[]): Status {
  let worst: Status = "good";
  for (const r of results) {
    if (r.error !== null && RANK.unknown > RANK[worst]) worst = "unknown";
    for (const i of r.issues) if (RANK[i.status] > RANK[worst]) worst = i.status;
  }
  return worst;
}

/** Detail pane for the category selected in the dock. */
export function CategoryPanel({
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
  const Icon = CATEGORY_ICONS[category];
  return (
    <section className="pane" data-testid={`panel-${category}`} data-status={status} aria-label={t(`category.${category}`)}>
      <header className="pane-head">
        <span className={`pane-icon s-${status}`}>
          <Icon size={26} aria-hidden />
        </span>
        <div className="pane-title">
          <h2>{t(`category.${category}`)}</h2>
          <span className="muted">{issues.length > 0 ? t("ui.results.issues_count", { count: issues.length }) : t(`status.${status}`)}</span>
        </div>
        <span className={`badge s-${status}`}>{t(`status.${status}`)}</span>
      </header>
      <div className="pane-body">
        {issues.map((i) => (
          <IssueItem key={i.id} issue={i} onFix={onFix} />
        ))}
        {failed.length > 0 && <p className="muted">{t("ui.results.could_not_check")}</p>}
        {issues.length === 0 && failed.length === 0 && <p className="muted">{t("ui.results.all_good")}</p>}
      </div>
    </section>
  );
}

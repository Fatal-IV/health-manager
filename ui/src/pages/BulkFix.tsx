import { ArrowRight } from "@phosphor-icons/react";
import type { ApplyResult, FixPlan } from "../api";
import { t } from "../i18n";
import { Outcome } from "./FixPreview";

export interface BulkItem {
  key: string;
  plan: FixPlan;
}

/**
 * One confirmation for several fixes: lists everything that will change, then
 * the outcome of each fix. Presentational: the caller runs the fixes one by one.
 */
export function BulkFix({
  items,
  busy,
  results,
  onConfirm,
  onCancel,
  onDone,
}: {
  items: BulkItem[];
  busy: boolean;
  results: ApplyResult[] | null;
  onConfirm: () => void;
  onCancel: () => void;
  onDone: () => void;
}) {
  const needsAdmin = items.some((i) => i.plan.needs_admin);
  const allReversible = items.every((i) => i.plan.reversible);
  return (
    <main>
      <h1>{t("ui.fix.bulk_title")}</h1>
      <p className="muted">{t("ui.fix.bulk_count", { count: items.length })}</p>
      {items.map((it, n) => (
        <section className="card" key={it.key}>
          <h2>{t(it.plan.summary_key, it.plan.params)}</h2>
          {!results && (
            <ul className="plain changes">
              {it.plan.changes.map((c) => (
                <li key={c}>
                  <ArrowRight size={16} aria-hidden />
                  <span>{t(c, it.plan.params)}</span>
                </li>
              ))}
            </ul>
          )}
          {results?.[n] && <Outcome result={results[n]} />}
        </section>
      ))}
      {!results && (
        <>
          {needsAdmin && <p>{t("ui.fix.bulk_admin")}</p>}
          <p className="muted">{allReversible ? t("ui.fix.reversible") : t("ui.fix.not_reversible")}</p>
          <div className="row">
            <button type="button" className="primary" disabled={busy} onClick={onConfirm}>
              {busy ? t("ui.fix.working") : t("ui.fix.confirm")}
            </button>
            <button type="button" disabled={busy} onClick={onCancel}>
              {t("ui.fix.cancel")}
            </button>
          </div>
        </>
      )}
      {results && (
        <button type="button" className="primary" onClick={onDone}>
          {t("ui.fix.done")}
        </button>
      )}
    </main>
  );
}

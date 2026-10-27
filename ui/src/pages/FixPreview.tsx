import { ArrowRight, CheckCircle, WarningCircle, XCircle } from "@phosphor-icons/react";
import type { ApplyResult, FixPlan } from "../api";
import { has, t } from "../i18n";

/** A fix may bring its own wording (`fix.<id>.<part>`); otherwise the general text is used. */
function own(fix: string | undefined, part: string, general: string): string {
  const key = fix ? `fix.${fix}.${part}` : "";
  return key && has(key) ? t(key) : t(general);
}

/**
 * Shows what a fix will change and asks for confirmation; after the fix ran it
 * shows the outcome. Presentational: the caller runs the fix.
 */
export function FixPreview({
  plan,
  busy,
  result,
  onConfirm,
  onCancel,
  onDone,
}: {
  plan: FixPlan;
  busy: boolean;
  result: ApplyResult | null;
  onConfirm: () => void;
  onCancel: () => void;
  onDone: () => void;
}) {
  const params = plan.params;
  return (
    <main>
      <h1>{own(plan.fix, "title", "ui.fix.title")}</h1>
      <section className="card">
        <h2>{t(plan.summary_key, params)}</h2>

        {!result && (
          <>
            <ul className="plain changes" aria-label={t("ui.fix.title")}>
              {plan.changes.map((c) => (
                <li key={c}>
                  <ArrowRight size={16} aria-hidden />
                  <span>{t(c, params)}</span>
                </li>
              ))}
            </ul>
            {plan.needs_admin && <p>{t("ui.fix.admin_note")}</p>}
            <p className="muted">{plan.reversible ? t("ui.fix.reversible") : t("ui.fix.not_reversible")}</p>
            <div className="row">
              <button type="button" className="primary" disabled={busy} onClick={onConfirm}>
                {busy ? own(plan.fix, "working", "ui.fix.working") : own(plan.fix, "confirm", "ui.fix.confirm")}
              </button>
              <button type="button" disabled={busy} onClick={onCancel}>
                {t("ui.fix.cancel")}
              </button>
            </div>
          </>
        )}

        {result && <Outcome result={result} fix={plan.fix} />}
        {result && (
          <button type="button" className="primary" onClick={onDone}>
            {t("ui.fix.done")}
          </button>
        )}
      </section>
    </main>
  );
}

export function Outcome({ result, fix }: { result: ApplyResult; fix?: string }) {
  const o = result.outcome;
  if (o.status === "declined") return <p role="status">{t("fix.result.declined")}</p>;
  if (o.status === "applied") {
    return (
      <div role="status" className="outcome">
        {result.verified ? (
          <CheckCircle size={20} weight="fill" className="s-good" aria-hidden />
        ) : (
          <WarningCircle size={20} weight="fill" className="s-warning" aria-hidden />
        )}
        <div>
          <p>{t(o.message, o.params)}</p>
          <p className={result.verified ? "s-good" : "s-warning"}>
            {result.verified ? own(fix, "verified", "ui.fix.verified") : own(fix, "unverified", "ui.fix.unverified")}
          </p>
        </div>
      </div>
    );
  }
  return (
    <div role="alert">
      <div className="outcome">
        <XCircle size={20} weight="fill" className="s-critical" aria-hidden />
        <p className="err">{t(o.message, o.params)}</p>
      </div>
      {o.detail && (
        <details>
          <summary>{t("ui.fix.technical")}</summary>
          <pre style={{ whiteSpace: "pre-wrap" }}>{o.detail}</pre>
        </details>
      )}
    </div>
  );
}

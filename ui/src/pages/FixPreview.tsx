import type { ApplyResult, FixPlan } from "../api";
import { t } from "../i18n";

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
      <h1>{t("ui.fix.title")}</h1>
      <section className="card">
        <h2>{t(plan.summary_key, params)}</h2>

        {!result && (
          <>
            <ul className="plain" aria-label={t("ui.fix.title")}>
              {plan.changes.map((c) => (
                <li key={c}>{t(c, params)}</li>
              ))}
            </ul>
            {plan.needs_admin && <p>{t("ui.fix.admin_note")}</p>}
            <p className="muted">{plan.reversible ? t("ui.fix.reversible") : t("ui.fix.not_reversible")}</p>
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

        {result && <Outcome result={result} />}
        {result && (
          <button type="button" className="primary" onClick={onDone}>
            {t("ui.fix.done")}
          </button>
        )}
      </section>
    </main>
  );
}

function Outcome({ result }: { result: ApplyResult }) {
  const o = result.outcome;
  if (o.status === "declined") return <p role="status">{t("fix.result.declined")}</p>;
  if (o.status === "applied") {
    return (
      <div role="status">
        <p>{t(o.message, o.params)}</p>
        <p className={result.verified ? "s-good" : "s-warning"}>
          {result.verified ? t("ui.fix.verified") : t("ui.fix.unverified")}
        </p>
      </div>
    );
  }
  return (
    <div role="alert">
      <p className="err">{t(o.message, o.params)}</p>
      {o.detail && (
        <details>
          <summary>{t("ui.fix.technical")}</summary>
          <pre style={{ whiteSpace: "pre-wrap" }}>{o.detail}</pre>
        </details>
      )}
    </div>
  );
}

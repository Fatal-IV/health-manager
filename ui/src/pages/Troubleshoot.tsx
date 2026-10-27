import { useCallback, useEffect, useState } from "react";
import { api, type ApplyResult, type FixPlan, type TroubleshooterId, type TroubleshootState } from "../api";
import { StepCard } from "../components/StepCard";
import { t } from "../i18n";
import { FixPreview } from "./FixPreview";

type Phase =
  | { name: "loading" }
  | { name: "error" }
  | { name: "state"; s: TroubleshootState }
  | { name: "fix"; s: TroubleshootState; plan: FixPlan; busy: boolean; result: ApplyResult | null }
  | { name: "retry"; s: TroubleshootState };

/**
 * One guided troubleshooter. The state machine lives in Rust; this page
 * only shows its answer. Every repair goes through the normal preview and
 * confirmation, one step at a time.
 */
export function Troubleshoot({ id, onBack }: { id: TroubleshooterId; onBack: () => void }) {
  const [phase, setPhase] = useState<Phase>({ name: "loading" });

  const show = useCallback((r: Awaited<ReturnType<typeof api.troubleshootStart>>) => {
    setPhase(r.ok ? { name: "state", s: r.value } : { name: "error" });
  }, []);

  useEffect(() => {
    let alive = true;
    void api.troubleshootStart(id).then((r) => alive && show(r));
    return () => {
      alive = false;
    };
  }, [id, show]);

  const openFix = async (s: TroubleshootState) => {
    if (!s.fix) return;
    const r = await api.getFixPlan(s.fix);
    if (!r.ok) return setPhase({ name: "error" });
    setPhase({ name: "fix", s, plan: r.value, busy: false, result: null });
  };

  const confirm = async () => {
    if (phase.name !== "fix" || phase.busy) return;
    const { s, plan } = phase;
    setPhase({ ...phase, busy: true });
    const r = await api.applyFix(plan.fix);
    const result: ApplyResult = r.ok
      ? r.value
      : { outcome: { status: "failed", message: "fix.result.failed", params: {}, detail: r.error }, verified: false, report: null };
    setPhase({ name: "fix", s, plan, busy: false, result });
  };

  const done = async () => {
    if (phase.name !== "fix" || !phase.result) return;
    const { s, result } = phase;
    // A declined or failed fix never reaches the re-test: nothing changed.
    if (result.outcome.status !== "applied") return setPhase({ name: "retry", s });
    setPhase({ name: "loading" });
    show(await api.troubleshootAfterFix(id, s.step ?? 0));
  };

  const skip = async (s: TroubleshootState) => {
    setPhase({ name: "loading" });
    show(await api.troubleshootSkip(id, s.step ?? 0));
  };

  if (phase.name === "fix") {
    return (
      <FixPreview
        plan={phase.plan}
        busy={phase.busy}
        result={phase.result}
        onConfirm={() => void confirm()}
        onCancel={() => setPhase({ name: "state", s: phase.s })}
        onDone={() => void done()}
        unverifiedKey="ui.troubleshoot.unverified"
      />
    );
  }

  return (
    <main>
      <h1>{t(`ui.troubleshoot.titles.${id}`)}</h1>
      {phase.name === "loading" && <p role="status">{t("ui.troubleshoot.loading")}</p>}
      {phase.name === "error" && <p role="alert">{t("ui.troubleshoot.error")}</p>}
      {phase.name === "retry" && (
        <section className="card">
          <h2>{t("ui.troubleshoot.declined_title")}</h2>
          <p>{t("ui.troubleshoot.declined_text")}</p>
          <div className="row">
            <button type="button" className="primary" onClick={() => setPhase({ name: "state", s: phase.s })}>
              {t("ui.troubleshoot.retry")}
            </button>
            <button type="button" onClick={() => void skip(phase.s)}>
              {t("ui.troubleshoot.skip")}
            </button>
          </div>
        </section>
      )}
      {phase.name === "state" && phase.s.state === "suggest" && (
        <StepCard state={phase.s} busy={false} onApply={() => void openFix(phase.s)} onSkip={() => void skip(phase.s)} />
      )}
      {phase.name === "state" && phase.s.state !== "suggest" && (
        <section className="card">
          <h2>{t(`ui.troubleshoot.${phase.s.state}_title`)}</h2>
          <p>{t(`ui.troubleshoot.texts.${id}.${phase.s.state}`)}</p>
          {phase.s.state === "resolved" && phase.s.reboot && <p>{t("ui.troubleshoot.reboot_note")}</p>}
        </section>
      )}
      {phase.name === "state" && phase.s.hints.length > 0 && (
        <ul className="plain" aria-label={t("ui.troubleshoot.hints_label")}>
          {phase.s.hints.map((h) => (
            <li key={h} className="muted">
              {t(`ui.troubleshoot.hints.${h}`)}
            </li>
          ))}
        </ul>
      )}
      <div className="row">
        <button type="button" onClick={onBack}>
          {t("ui.troubleshoot.back")}
        </button>
      </div>
    </main>
  );
}

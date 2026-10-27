import { Wrench } from "@phosphor-icons/react";
import type { TroubleshootState } from "../api";
import { t } from "../i18n";

/** The proposed repair step: what it is, why it is next, and the choice to apply or skip it. */
export function StepCard({
  state,
  onApply,
  onSkip,
  busy,
}: {
  state: TroubleshootState;
  onApply: () => void;
  onSkip: () => void;
  busy: boolean;
}) {
  const fix = state.fix ?? "";
  return (
    <section className="card">
      <p className="eyebrow">{t("ui.troubleshoot.step", { n: (state.step ?? 0) + 1, total: state.total })}</p>
      <h2>{t(`fix.${fix}.summary`)}</h2>
      <p>{t(`ui.troubleshoot.why.${fix}`)}</p>
      {state.unmeasured && <p className="muted">{t("ui.troubleshoot.unmeasured")}</p>}
      <div className="row">
        <button type="button" className="primary" disabled={busy} onClick={onApply}>
          <Wrench size={17} aria-hidden />
          {t("ui.troubleshoot.apply")}
        </button>
        <button type="button" disabled={busy} onClick={onSkip}>
          {t("ui.troubleshoot.skip")}
        </button>
      </div>
    </section>
  );
}

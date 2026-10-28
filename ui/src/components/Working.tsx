import { CheckCircle, Circle, SpinnerGap } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { api, type FixPlan, type ProgressMode } from "../api";
import { t } from "../i18n";

/** Staged mode: how often the bar closes half of its remaining distance to the stage's target. */
const EASE_MS = 1000;

/**
 * The waiting screen of a troubleshooter while its fix runs and while the result is
 * re-tested: which fix, which step, the operations and a progress bar. The bar has one
 * unit per operation plus one for the re-test. `live` follows the helper's reports;
 * `staged` only knows the stage and eases toward its target.
 */
export function Working({
  title,
  step,
  total,
  plan,
  stage,
  mode,
}: {
  title: string;
  step: number;
  total: number;
  plan: FixPlan;
  stage: "applying" | "checking";
  mode: ProgressMode;
}) {
  const n = plan.ops.length;
  const [done, setDone] = useState(0);
  const [eased, setEased] = useState(0);

  useEffect(() => {
    if (mode !== "live") return;
    let alive = true;
    let stop: (() => void) | undefined;
    void api.onFixProgress((p) => alive && setDone((d) => Math.max(d, Math.min(p.done, n)))).then((un) => {
      if (alive) stop = un;
      else un();
    });
    return () => {
      alive = false;
      stop?.();
    };
  }, [mode, n]);

  const target = stage === "applying" ? n / (n + 1) : 1;
  useEffect(() => {
    if (mode !== "staged") return;
    // Entering the re-test starts at least at the end of the operations.
    if (stage === "checking") setEased((e) => Math.max(e, n / (n + 1)));
    const timer = setInterval(() => setEased((e) => e + (target - e) / 2), EASE_MS);
    return () => clearInterval(timer);
  }, [mode, stage, target, n]);

  const finished = stage === "checking" ? n : mode === "live" ? done : 0;
  const state = (i: number) => (i < finished ? "done" : i === finished ? "current" : "pending");
  const fraction = mode === "live" ? finished / (n + 1) : eased;
  const percent = Math.floor(Math.min(fraction, 1) * 100);

  return (
    <main>
      <h1>{title}</h1>
      <section className="card working">
        <p className="eyebrow">{t("ui.troubleshoot.step", { n: step + 1, total })}</p>
        <h2>{t(plan.summary_key, plan.params)}</h2>
        <p role="status">{t(`ui.working.${stage}`)}</p>
        <div
          className="working-bar"
          role="progressbar"
          aria-label={t("ui.working.progress")}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
        >
          <span style={{ width: `${percent}%` }} />
        </div>
        <ol className="plain working-ops">
          {plan.ops.map((key, i) => (
            <li key={key} data-state={state(i)}>
              {state(i) === "done" ? (
                <CheckCircle size={16} weight="fill" aria-hidden />
              ) : state(i) === "current" ? (
                <SpinnerGap size={16} className="spin" aria-hidden />
              ) : (
                <Circle size={16} aria-hidden />
              )}
              <span>{t(key, plan.params)}</span>
            </li>
          ))}
        </ol>
        {stage === "applying" && plan.needs_admin && <p className="muted">{t("ui.working.admin_note")}</p>}
      </section>
    </main>
  );
}

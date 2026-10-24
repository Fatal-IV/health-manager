import { useState } from "react";
import { api, type AppliedFix, type ApplyResult, type FixPlan, type Issue, type ScanMode, type ScanReport } from "./api";
import { t } from "./i18n";
import { FixPreview } from "./pages/FixPreview";
import { Home } from "./pages/Home";
import { Report } from "./pages/Report";
import { Results } from "./pages/Results";
import { Scanning } from "./pages/Scanning";

type View =
  | { name: "home" }
  | { name: "scanning"; mode: ScanMode; run: number }
  | { name: "results" }
  | { name: "fix"; fix: string; arg?: string; plan: FixPlan; busy: boolean; result: ApplyResult | null }
  | { name: "report" };

export default function App() {
  const [view, setView] = useState<View>({ name: "home" });
  const [report, setReport] = useState<ScanReport | null>(null);
  const [applied, setApplied] = useState<AppliedFix[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [runs, setRuns] = useState(0);

  const startScan = (mode: ScanMode) => {
    setNotice(null);
    setRuns((n) => n + 1);
    setView({ name: "scanning", mode, run: runs + 1 });
  };

  const openFix = async (issue: Issue, arg?: string, label?: string) => {
    if (!issue.fix) return;
    setNotice(null);
    const r = await api.getFixPlan(issue.fix, arg);
    if (!r.ok) {
      setNotice(t("fix.result.invalid"));
      return;
    }
    // The preview should name the program, not its internal id.
    const plan = label ? { ...r.value, params: { ...r.value.params, target: label } } : r.value;
    setView({ name: "fix", fix: issue.fix, arg, plan, busy: false, result: null });
  };

  const confirmFix = async () => {
    if (view.name !== "fix" || view.busy) return;
    const { fix, arg } = view;
    setView({ ...view, busy: true });
    const r = await api.applyFix(fix, arg);
    const result: ApplyResult = r.ok
      ? r.value
      : { outcome: { status: "failed", message: "fix.result.failed", params: {}, detail: r.error }, verified: false, report: null };
    if (result.report) setReport(result.report);
    const s = await api.getSession();
    if (s.ok) setApplied(s.value.applied);
    setView((v) => (v.name === "fix" ? { ...v, busy: false, result } : v));
  };

  return (
    <>
      <header style={{ padding: "12px 20px" }} className="muted">
        {t("app.title")}
      </header>
      {notice && (
        <p role="alert" className="err" style={{ textAlign: "center" }}>
          {notice}
        </p>
      )}
      {view.name === "home" && <Home onStart={startScan} lastScan={report?.scanned_at} score={report && !report.cancelled ? report.score.value : null} />}
      {view.name === "scanning" && (
        <Scanning
          key={view.run}
          mode={view.mode}
          onDone={(r) => {
            setReport(r);
            setApplied([]);
            setView({ name: "results" });
          }}
          onCancel={() => setView({ name: "home" })}
        />
      )}
      {view.name === "results" && report && (
        <Results
          report={report}
          onFix={(issue, arg, label) => void openFix(issue, arg, label)}
          onReport={() => setView({ name: "report" })}
          onRescan={() => startScan("quick")}
        />
      )}
      {view.name === "fix" && (
        <FixPreview
          plan={view.plan}
          busy={view.busy}
          result={view.result}
          onConfirm={() => void confirmFix()}
          onCancel={() => setView({ name: "results" })}
          onDone={() => setView({ name: "results" })}
        />
      )}
      {view.name === "report" && report && <Report report={report} applied={applied} onBack={() => setView({ name: "results" })} />}
    </>
  );
}

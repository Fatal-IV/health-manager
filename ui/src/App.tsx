import { useState } from "react";
import { api, type AppliedFix, type Category, type ApplyResult, type FixPlan, type Issue, type ScanMode, type ScanReport } from "./api";
import { TitleBar } from "./components/TitleBar";
import { UpdateBar } from "./components/UpdateBar";
import { t } from "./i18n";
import { BulkFix, type BulkItem } from "./pages/BulkFix";
import { FixPreview } from "./pages/FixPreview";
import { Home } from "./pages/Home";
import { Report } from "./pages/Report";
import { Results } from "./pages/Results";
import { Scanning } from "./pages/Scanning";

type View =
  | { name: "home" }
  | { name: "scanning"; mode: ScanMode; run: number; categories?: Category[] }
  | { name: "results"; focus?: Category }
  | { name: "bulk"; items: BulkItem[]; busy: boolean; results: ApplyResult[] | null }
  | { name: "fix"; fix: string; arg?: string; plan: FixPlan; busy: boolean; result: ApplyResult | null }
  | { name: "report" };

export default function App() {
  const [view, setView] = useState<View>({ name: "home" });
  const [report, setReport] = useState<ScanReport | null>(null);
  const [applied, setApplied] = useState<AppliedFix[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [runs, setRuns] = useState(0);

  const startScan = (mode: ScanMode, categories?: Category[]) => {
    setNotice(null);
    setRuns((n) => n + 1);
    setView({ name: "scanning", mode, run: runs + 1, categories });
  };

  const openBulk = async (issues: Issue[]) => {
    setNotice(null);
    const items: BulkItem[] = [];
    for (const issue of issues) {
      if (!issue.fix) continue;
      const r = await api.getFixPlan(issue.fix);
      if (!r.ok) {
        setNotice(t("fix.result.invalid"));
        return;
      }
      items.push({ key: issue.id, plan: r.value });
    }
    if (items.length > 0) setView({ name: "bulk", items, busy: false, results: null });
  };

  const confirmBulk = async () => {
    if (view.name !== "bulk" || view.busy) return;
    const { items } = view;
    setView({ ...view, busy: true });
    const results: ApplyResult[] = [];
    // One by one: each admin fix gets its own Windows permission prompt.
    for (const it of items) {
      const r = await api.applyFix(it.plan.fix);
      const result: ApplyResult = r.ok
        ? r.value
        : { outcome: { status: "failed", message: "fix.result.failed", params: {}, detail: r.error }, verified: false, report: null };
      if (result.report) setReport(result.report);
      results.push(result);
    }
    const s = await api.getSession();
    if (s.ok) setApplied(s.value.applied);
    setView({ name: "bulk", items, busy: false, results });
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
      <TitleBar />
      <UpdateBar />
      {notice && (
        <p role="alert" className="notice-bar">
          {notice}
        </p>
      )}
      {view.name === "home" && <Home onStart={startScan} lastScan={report?.scanned_at} score={report && !report.cancelled ? report.score.value : null} />}
      {view.name === "scanning" && (
        <Scanning
          key={view.run}
          mode={view.mode}
          categories={view.categories}
          onDone={(r) => {
            setReport(r);
            // A full scan starts a new session; a single-area re-scan keeps the applied fixes.
            if (!view.categories) setApplied([]);
            setView({ name: "results", focus: view.categories?.[0] });
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
          onRescanArea={(c) => startScan("quick", [c])}
          onBulkFix={(issues) => void openBulk(issues)}
          initial={view.focus}
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
      {view.name === "bulk" && (
        <BulkFix
          items={view.items}
          busy={view.busy}
          results={view.results}
          onConfirm={() => void confirmBulk()}
          onCancel={() => setView({ name: "results" })}
          onDone={() => setView({ name: "results" })}
        />
      )}
      {view.name === "report" && report && <Report report={report} applied={applied} onBack={() => setView({ name: "results" })} />}
    </>
  );
}

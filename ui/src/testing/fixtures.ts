import type { ApplyResult, CheckResult, FixPlan, Issue, ScanReport } from "../api";

export function issue(over: Partial<Issue> = {}): Issue {
  return {
    id: "storage.temp_files",
    category: "storage",
    status: "warning",
    title_key: "issue.storage.temp_files.title",
    why_key: "issue.storage.temp_files.why",
    evidence: "temp 2.0 GiB",
    params: { size_gb: "2.0" },
    fix: "clean_temp",
    ...over,
  };
}

export function check(over: Partial<CheckResult> = {}): CheckResult {
  return { check_id: "storage.temp_files", category: "storage", issues: [issue()], error: null, ...over };
}

export function report(over: Partial<ScanReport> = {}): ScanReport {
  return {
    results: [
      check(),
      check({
        check_id: "security.defender",
        category: "security",
        issues: [],
      }),
    ],
    score: { value: 95, breakdown: [{ issue_id: "storage.temp_files", penalty: 5 }] },
    cancelled: false,
    scanned_at: "2026-10-24T09:30:00Z",
    ...over,
  };
}

export function plan(over: Partial<FixPlan> = {}): FixPlan {
  return {
    fix: "clean_temp",
    summary_key: "fix.clean_temp.summary",
    changes: ["fix.clean_temp.change.1", "fix.clean_temp.change.2"],
    params: {},
    needs_admin: false,
    reversible: false,
    ops: ["fix.clean_temp.op.1"],
    ...over,
  };
}

export function applied(over: Partial<ApplyResult> = {}): ApplyResult {
  return {
    outcome: { status: "applied", message: "fix.result.ok", params: {} },
    verified: true,
    report: null,
    ...over,
  };
}

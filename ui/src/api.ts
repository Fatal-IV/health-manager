import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Category = "storage" | "performance" | "update" | "security" | "system";
export type Status = "good" | "warning" | "critical" | "unknown";
export type ScanMode = "quick" | "deep";
export type ReportFormat = "text" | "pdf";

export interface Issue {
  id: string;
  category: Category;
  status: Status;
  title_key: string;
  why_key: string;
  evidence: string;
  params: Record<string, string>;
  fix: string | null;
}
export interface CheckResult {
  check_id: string;
  category: Category;
  issues: Issue[];
  error: string | null;
}
export interface Score {
  value: number;
  breakdown: { issue_id: string; penalty: number }[];
}
export interface ScanReport {
  results: CheckResult[];
  score: Score;
  cancelled: boolean;
  scanned_at: string;
}
export interface FixPlan {
  fix: string;
  summary_key: string;
  changes: string[];
  params: Record<string, string>;
  needs_admin: boolean;
  reversible: boolean;
}
export type FixOutcome =
  | { status: "applied"; message: string; params: Record<string, string> }
  | { status: "declined" }
  | { status: "failed"; message: string; params: Record<string, string>; detail: string };
export interface ApplyResult {
  outcome: FixOutcome;
  verified: boolean;
  report: ScanReport | null;
}
export interface AppliedFix {
  fix: string;
  outcome: "applied" | "unverified";
  at: string;
}
export interface Session {
  report: ScanReport | null;
  applied: AppliedFix[];
}
export interface Progress {
  done: number;
  total: number;
  category: Category;
}

/** What the start screen shows before any scan; a value that could not be read is null. */
export interface SystemSummary {
  os: string | null;
  uptime_secs: number | null;
  boot_secs: number | null;
  disk: { mount: string; free_bytes: number; total_bytes: number } | null;
  memory: { used_bytes: number; total_bytes: number } | null;
}

/** An available newer release; the download links stay on the Rust side. */
export interface UpdateInfo {
  version: string;
  notes: string;
}

/** What the running version brings, as its release describes it. */
export interface ReleaseNotes {
  version: string;
  notes: string;
}

export type Result<T> = { ok: true; value: T } | { ok: false; error: string };

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<Result<T>> {
  try {
    return { ok: true, value: await invoke<T>(cmd, args) };
  } catch (e) {
    return { ok: false, error: typeof e === "string" ? e : e instanceof Error ? e.message : String(e) };
  }
}

/** Every troubleshooter, in display order. A Rust test keeps this list equal to `TroubleshooterId::ALL`. */
export const TROUBLESHOOTERS = ["network", "windows_update", "system_files", "slowness", "audio", "printer", "bluetooth"] as const;
export type TroubleshooterId = (typeof TROUBLESHOOTERS)[number];

export interface TroubleshootState {
  state: "healthy" | "suggest" | "resolved" | "exhausted";
  step: number | null;
  total: number;
  fix: string | null;
  reboot: boolean;
  unmeasured: boolean;
  /** Keys of readings no step can fix (`high_ram`, `high_cpu`). */
  hints: string[];
  /** False when the troubleshooter has no quick test and always offers its repair. */
  tested: boolean;
}

export const api = {
  startScan: (mode: ScanMode, categories?: Category[]) =>
    call<ScanReport>("start_scan", { mode, categories: categories ?? null }),
  cancelScan: () => call<void>("cancel_scan"),
  getSystemSummary: () => call<SystemSummary>("get_system_summary"),
  checkUpdate: () => call<UpdateInfo | null>("check_update"),
  installUpdate: () => call<void>("install_update"),
  getReleaseNotes: () => call<ReleaseNotes | null>("get_release_notes"),
  openWindowsUpdate: () => call<void>("open_windows_update"),
  getFixPlan: (fix: string, arg?: string) => call<FixPlan>("get_fix_plan", { fix, arg: arg ?? null }),
  applyFix: (fix: string, arg?: string) => call<ApplyResult>("apply_fix_cmd", { fix, arg: arg ?? null }),
  getSession: () => call<Session>("get_session"),
  troubleshootStart: (id: TroubleshooterId) => call<TroubleshootState>("troubleshoot_start", { id }),
  troubleshootAfterFix: (id: TroubleshooterId, step: number) => call<TroubleshootState>("troubleshoot_after_fix", { id, step }),
  troubleshootSkip: (id: TroubleshooterId, step: number) => call<TroubleshootState>("troubleshoot_skip", { id, step }),
  exportReport: (format: ReportFormat, path: string) => call<void>("export_report", { format, path }),
  onProgress: (cb: (p: Progress) => void) => listen<Progress>("scan-progress", (e) => cb(e.payload)),
};

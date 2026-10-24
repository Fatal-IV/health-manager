import { invoke } from "@tauri-apps/api/core";
import { api } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const mocked = vi.mocked(invoke);

describe("api", () => {
  beforeEach(() => mocked.mockReset());

  it("maps invoke errors to a typed result", async () => {
    mocked.mockRejectedValueOnce("scan_in_progress");
    expect(await api.startScan("quick")).toEqual({ ok: false, error: "scan_in_progress" });
    mocked.mockRejectedValueOnce(new Error("boom"));
    expect(await api.cancelScan()).toEqual({ ok: false, error: "boom" });
  });

  it("returns the value on success", async () => {
    mocked.mockResolvedValueOnce({ outcome: { status: "declined" }, verified: false, report: null });
    const r = await api.applyFix("clean_temp");
    expect(r).toEqual({ ok: true, value: { outcome: { status: "declined" }, verified: false, report: null } });
  });

  it("sends the command names and argument shapes the Rust side expects", async () => {
    mocked.mockResolvedValue(undefined);
    await api.startScan("deep");
    await api.getFixPlan("disable_startup_item", "HKCU|a");
    await api.getFixPlan("clean_temp");
    await api.exportReport("pdf", "C:\\x\\r.pdf");
    expect(mocked.mock.calls).toEqual([
      ["start_scan", { mode: "deep" }],
      ["get_fix_plan", { fix: "disable_startup_item", arg: "HKCU|a" }],
      ["get_fix_plan", { fix: "clean_temp", arg: null }],
      ["export_report", { format: "pdf", path: "C:\\x\\r.pdf" }],
    ]);
  });
});

import { act, fireEvent, render, screen } from "@testing-library/react";
import App from "./App";
import { api } from "./api";
import { t } from "./i18n";
import { applied, check, issue, plan, report } from "./testing/fixtures";

vi.mock("./api", async (orig) => {
  const real = await orig<typeof import("./api")>();
  return {
    ...real,
    api: {
      startScan: vi.fn(),
      cancelScan: vi.fn(),
      getFixPlan: vi.fn(),
      applyFix: vi.fn(),
      getSession: vi.fn(),
      exportReport: vi.fn(),
      onProgress: vi.fn(),
      checkUpdate: vi.fn(),
      installUpdate: vi.fn(),
    },
  };
});
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const flush = () => act(async () => undefined);

describe("App", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.setItem("saglik-denetimi.welcomed", "1");
    vi.mocked(api.onProgress).mockResolvedValue(() => undefined);
    vi.mocked(api.cancelScan).mockResolvedValue({ ok: true, value: undefined });
    vi.mocked(api.checkUpdate).mockResolvedValue({ ok: true, value: null });
  });

  it("renders app title and the start screen", () => {
    render(<App />);
    expect(screen.getByText(t("app.title"))).toBeInTheDocument();
    expect(screen.getByRole("button", { name: t("ui.home.start") })).toBeInTheDocument();
  });

  it("batch: pick two fixes, one preview, one confirmation, each fix applied in turn", async () => {
    const a = issue({ id: "a", fix: "clean_temp" });
    const b = issue({ id: "b", fix: "empty_recycle_bin" });
    vi.mocked(api.startScan).mockResolvedValue({ ok: true, value: report({ results: [check({ issues: [a, b] })] }) });
    vi.mocked(api.getFixPlan).mockImplementation(async (fix) => ({ ok: true, value: plan({ fix }) }));
    vi.mocked(api.applyFix).mockResolvedValue({ ok: true, value: applied({ verified: true, report: null }) });
    vi.mocked(api.getSession).mockResolvedValue({ ok: true, value: { report: null, applied: [] } });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    await flush();
    await screen.findByText(t("ui.results.title"));
    for (const box of screen.getAllByRole("checkbox")) fireEvent.click(box);
    fireEvent.click(screen.getByRole("button", { name: `${t("ui.results.fix_selected")} (2)` }));
    await flush();
    expect(await screen.findByText(t("ui.fix.bulk_title"))).toBeInTheDocument();
    expect(api.applyFix).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: t("ui.fix.confirm") }));
    await flush();
    await flush();
    expect(vi.mocked(api.applyFix).mock.calls.map((c) => c[0])).toEqual(["clean_temp", "empty_recycle_bin"]);
    expect(screen.getAllByText(t("ui.fix.verified"))).toHaveLength(2);
  });

  it("re-scanning one area asks for just that area and keeps the results view", async () => {
    vi.mocked(api.startScan).mockResolvedValue({ ok: true, value: report() });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    await flush();
    await screen.findByText(t("ui.results.title"));
    fireEvent.click(screen.getByRole("button", { name: t("ui.results.rescan_area") }));
    await flush();
    expect(api.startScan).toHaveBeenLastCalledWith("quick", ["storage"]);
    expect(await screen.findByText(t("ui.results.title"))).toBeInTheDocument();
  });

  it("scan -> preview -> confirm -> verified -> results shows the updated report", async () => {
    const before = report();
    const after = report({
      results: [check({ issues: [] })],
      score: { value: 100, breakdown: [] },
    });
    vi.mocked(api.startScan).mockResolvedValue({ ok: true, value: before });
    vi.mocked(api.getFixPlan).mockResolvedValue({ ok: true, value: plan() });
    vi.mocked(api.applyFix).mockResolvedValue({ ok: true, value: applied({ verified: true, report: after }) });
    vi.mocked(api.getSession).mockResolvedValue({
      ok: true,
      value: { report: after, applied: [{ fix: "clean_temp", outcome: "applied", at: "2026-10-24T09:31:00Z" }] },
    });

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    await flush();
    expect(await screen.findByText(t("ui.results.title"))).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: t("ui.results.fix") }));
    await flush();
    expect(api.getFixPlan).toHaveBeenCalledWith("clean_temp", undefined);
    expect(api.applyFix).not.toHaveBeenCalled(); // nothing runs before the confirmation
    expect(screen.getByText(t("fix.clean_temp.change.1"))).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: t("ui.fix.confirm") }));
    await flush();
    expect(api.applyFix).toHaveBeenCalledWith("clean_temp", undefined);
    expect(screen.getByText(t("ui.fix.verified"))).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: t("ui.fix.done") }));
    expect(screen.getByRole("img")).toHaveAttribute("aria-label", `${t("ui.results.score")}: 100`);

    fireEvent.click(screen.getByRole("button", { name: t("ui.results.see_report") }));
    expect(screen.getByText(new RegExp(t("fix.clean_temp.summary")))).toBeInTheDocument();
  });

  it("passes the startup item id as the argument and names it in the preview", async () => {
    const items = JSON.stringify([{ id: "HKCU|Foo/Bar", name: "Foo" }]);
    const startup = check({
      check_id: "performance.startup_items",
      category: "performance",
      issues: [
        {
          id: "performance.startup_items",
          category: "performance",
          status: "warning",
          title_key: "issue.performance.startup_items.title",
          why_key: "issue.performance.startup_items.why",
          evidence: "",
          params: { count: "12", items },
          fix: "disable_startup_item",
        },
      ],
    });
    vi.mocked(api.startScan).mockResolvedValue({ ok: true, value: report({ results: [startup] }) });
    vi.mocked(api.getFixPlan).mockResolvedValue({
      ok: true,
      value: plan({
        fix: "disable_startup_item",
        summary_key: "fix.disable_startup_item.summary",
        changes: ["fix.disable_startup_item.change.1"],
        params: { target: "HKCU|Foo/Bar" },
      }),
    });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    await flush();
    fireEvent.click(await screen.findByRole("button", { name: `${t("ui.results.fix_item")}: Foo` }));
    await flush();
    expect(api.getFixPlan).toHaveBeenCalledWith("disable_startup_item", "HKCU|Foo/Bar");
    // the preview shows the friendly name, not the internal id
    expect(screen.getByText(/Foo/)).toBeInTheDocument();
    expect(screen.queryByText(/HKCU\|/)).not.toBeInTheDocument();
  });

  it("an invalid fix request shows a message and stays on the results", async () => {
    vi.mocked(api.startScan).mockResolvedValue({ ok: true, value: report() });
    vi.mocked(api.getFixPlan).mockResolvedValue({ ok: false, error: "bad" });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    await flush();
    fireEvent.click(await screen.findByRole("button", { name: t("ui.results.fix") }));
    await flush();
    expect(screen.getByRole("alert")).toHaveTextContent(t("fix.result.invalid"));
    expect(screen.getByText(t("ui.results.title"))).toBeInTheDocument();
  });

  it("a launcher error becomes a failed outcome, not a crash", async () => {
    vi.mocked(api.startScan).mockResolvedValue({ ok: true, value: report() });
    vi.mocked(api.getFixPlan).mockResolvedValue({ ok: true, value: plan() });
    vi.mocked(api.applyFix).mockResolvedValue({ ok: false, error: "ipc down" });
    vi.mocked(api.getSession).mockResolvedValue({ ok: true, value: { report: report(), applied: [] } });
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    await flush();
    fireEvent.click(await screen.findByRole("button", { name: t("ui.results.fix") }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: t("ui.fix.confirm") }));
    await flush();
    expect(screen.getByRole("alert")).toHaveTextContent(t("fix.result.failed"));
  });

  it("cancelling a scan returns to the start screen", async () => {
    vi.mocked(api.startScan).mockImplementation(() => new Promise(() => undefined));
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: t("ui.scanning.cancel") }));
    expect(screen.getByRole("button", { name: t("ui.home.start") })).toBeInTheDocument();
  });
});

import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { api, type SystemSummary } from "../api";
import { t } from "../i18n";
import { Home, summaryTiles } from "./Home";

const GIB = 1024 ** 3;
const flush = () => act(async () => undefined);
const full: SystemSummary = {
  os: "Windows 11 Home Single Language",
  uptime_secs: 3 * 86400,
  boot_secs: 48,
  disk: { mount: "C:", free_bytes: 142 * GIB, total_bytes: 237 * GIB },
  memory: { used_bytes: 8 * GIB, total_bytes: 16 * GIB },
};

const openOptions = () => fireEvent.click(screen.getByRole("button", { name: t("ui.home.edit") }));

describe("Home", () => {
  beforeEach(() => {
    vi.spyOn(api, "getSystemSummary").mockResolvedValue({ ok: false, error: "none" });
  });
  afterEach(() => vi.restoreAllMocks());

  it("says up front that nothing changes without approval", () => {
    render(<Home onStart={() => undefined} />);
    expect(screen.getByText(t("ui.home.trust"))).toBeInTheDocument();
  });

  it("offers the troubleshooter only when the app can open it", () => {
    const { rerender } = render(<Home onStart={() => undefined} />);
    expect(screen.queryByRole("button", { name: new RegExp(t("ui.home.troubleshoot")) })).not.toBeInTheDocument();
    const open = vi.fn();
    rerender(<Home onStart={() => undefined} onTroubleshoot={open} />);
    fireEvent.click(screen.getByRole("button", { name: new RegExp(t("ui.home.troubleshoot")) }));
    expect(open).toHaveBeenCalledTimes(1);
  });

  it("keeps the three actions in a dock, each named, each doing its own thing", () => {
    const [start, troubleshoot, both] = [vi.fn(), vi.fn(), vi.fn()];
    render(<Home onStart={start} onTroubleshoot={troubleshoot} onBoth={both} />);
    const dock = screen.getByRole("navigation", { name: t("ui.home.actions") });
    fireEvent.click(within(dock).getByRole("button", { name: t("ui.home.audit_title") }));
    expect(start).toHaveBeenCalledWith("quick");
    fireEvent.click(within(dock).getByRole("button", { name: t("ui.home.troubleshoot") }));
    expect(troubleshoot).toHaveBeenCalledTimes(1);
    fireEvent.click(within(dock).getByRole("button", { name: t("ui.home.both") }));
    expect(both).toHaveBeenCalledTimes(1);
  });

  it("offers the combined check only when the app can run it", () => {
    const { rerender } = render(<Home onStart={() => undefined} />);
    expect(screen.queryByRole("button", { name: t("ui.home.both") })).not.toBeInTheDocument();
    const both = vi.fn();
    rerender(<Home onStart={() => undefined} onBoth={both} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.both") }));
    expect(both).toHaveBeenCalledTimes(1);
  });

  it("shows the computer summary once it has been read", async () => {
    vi.mocked(api.getSystemSummary).mockResolvedValue({ ok: true, value: full });
    render(<Home onStart={() => undefined} />);
    await flush();
    const list = screen.getByRole("list", { name: t("ui.home.summary") });
    expect(list).toHaveTextContent("11 Home");
    expect(list).toHaveTextContent("3 gün");
    expect(list).toHaveTextContent("142 GB");
    expect(list).toHaveTextContent("%50");
  });

  it("a fact that could not be read is left out, and no summary at all is no empty box", async () => {
    expect(summaryTiles({ os: null, uptime_secs: null, boot_secs: null, disk: null, memory: null })).toEqual([]);
    expect(summaryTiles({ ...full, disk: null, memory: null }).map((x) => x.key)).toEqual(["os", "uptime", "boot"]);
    expect(summaryTiles({ ...full, boot_secs: null }).map((x) => x.key)).not.toContain("boot");
    render(<Home onStart={() => undefined} />);
    await flush();
    expect(screen.queryByRole("list", { name: t("ui.home.summary") })).not.toBeInTheDocument();
  });

  it("start button asks for a quick scan", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    expect(onStart).toHaveBeenCalledWith("quick");
  });

  it("areas and the deep option are tucked away until asked for", () => {
    render(<Home onStart={() => undefined} />);
    expect(screen.queryByLabelText(t("ui.home.deep"))).not.toBeInTheDocument();
    expect(screen.getByText(new RegExp(t("ui.home.areas_count", { count: 5 })))).toBeInTheDocument();
    openOptions();
    expect(screen.getByLabelText(t("ui.home.deep"))).toBeInTheDocument();
  });

  it("deep scan option asks for a deep scan", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    openOptions();
    fireEvent.click(screen.getByLabelText(t("ui.home.deep")));
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    expect(onStart).toHaveBeenCalledWith("deep");
  });

  it("a subset of areas is passed on, in a stable order", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    openOptions();
    fireEvent.click(screen.getByLabelText(t("category.update")));
    fireEvent.click(screen.getByLabelText(t("category.system")));
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    expect(onStart).toHaveBeenCalledWith("quick", ["security", "storage", "performance"]);
  });

  it("with no area selected the scan cannot start", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    openOptions();
    for (const c of ["security", "update", "storage", "performance", "system"]) fireEvent.click(screen.getByLabelText(t(`category.${c}`)));
    expect(screen.getByRole("button", { name: t("ui.home.start") })).toBeDisabled();
    expect(screen.getByText(t("ui.home.pick_one"))).toBeInTheDocument();
  });

  it("says 'never scanned' before the first scan, the last score and time after", () => {
    const { rerender } = render(<Home onStart={() => undefined} />);
    expect(screen.getByText(new RegExp(t("ui.home.never")))).toBeInTheDocument();
    rerender(<Home onStart={() => undefined} score={87} lastScan="2026-10-24T09:30:00Z" />);
    expect(screen.queryByText(new RegExp(t("ui.home.never")))).not.toBeInTheDocument();
    expect(screen.getByText(new RegExp(t("ui.home.score_line", { score: 87 })))).toBeInTheDocument();
  });
});

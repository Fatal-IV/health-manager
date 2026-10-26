import { act, fireEvent, render, screen } from "@testing-library/react";
import { StrictMode } from "react";
import { api, type Progress, type Result, type ScanReport } from "../api";
import { t } from "../i18n";
import { report } from "../testing/fixtures";
import { Scanning } from "./Scanning";

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, api: { startScan: vi.fn(), cancelScan: vi.fn(), onProgress: vi.fn() } };
});

const flush = () => act(async () => undefined);

function setup() {
  let emit: (p: Progress) => void = () => undefined;
  let finish: (r: Result<ScanReport>) => void = () => undefined;
  vi.mocked(api.onProgress).mockImplementation(async (cb) => {
    emit = cb;
    return () => undefined;
  });
  vi.mocked(api.startScan).mockImplementation(() => new Promise((res) => (finish = res)));
  vi.mocked(api.cancelScan).mockResolvedValue({ ok: true, value: undefined });
  return { emit: (p: Progress) => act(() => emit(p)), finish: (r: Result<ScanReport>) => act(async () => finish(r)) };
}

describe("Scanning", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows category progress from events", async () => {
    const h = setup();
    render(<Scanning mode="quick" onDone={() => undefined} onCancel={() => undefined} />);
    await flush();
    expect(screen.getByText(t("ui.scanning.starting"))).toBeInTheDocument();
    await h.emit({ done: 3, total: 12, category: "update" });
    expect(screen.getByText(/3 \/ 12/)).toBeInTheDocument();
    expect(screen.getByText(new RegExp(t("category.update")))).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("value", "25");
  });

  it("hands the finished report to onDone", async () => {
    const h = setup();
    const onDone = vi.fn();
    render(<Scanning mode="deep" onDone={onDone} onCancel={() => undefined} />);
    await flush();
    expect(api.startScan).toHaveBeenCalledWith("deep");
    await h.finish({ ok: true, value: report() });
    expect(onDone).toHaveBeenCalledWith(report());
  });

  it("passes the chosen areas to the scan", async () => {
    setup();
    render(<Scanning mode="quick" categories={["storage"]} onDone={() => undefined} onCancel={() => undefined} />);
    await flush();
    expect(api.startScan).toHaveBeenCalledWith("quick", ["storage"]);
  });

  it("cancel stops the scan, returns home, and a late result is ignored", async () => {
    const h = setup();
    const onDone = vi.fn();
    const onCancel = vi.fn();
    render(<Scanning mode="quick" onDone={onDone} onCancel={onCancel} />);
    await flush();
    fireEvent.click(screen.getByRole("button", { name: t("ui.scanning.cancel") }));
    expect(api.cancelScan).toHaveBeenCalledTimes(1);
    expect(onCancel).toHaveBeenCalledTimes(1);
    await h.finish({ ok: true, value: report({ cancelled: true }) });
    expect(onDone).not.toHaveBeenCalled();
  });

  it("starts exactly one scan under React StrictMode", async () => {
    setup();
    render(
      <StrictMode>
        <Scanning mode="quick" onDone={() => undefined} onCancel={() => undefined} />
      </StrictMode>,
    );
    await flush();
    expect(api.startScan).toHaveBeenCalledTimes(1);
  });

  it("tells the user when a scan is already running or could not start", async () => {
    const h = setup();
    render(<Scanning mode="quick" onDone={() => undefined} onCancel={() => undefined} />);
    await flush();
    await h.finish({ ok: false, error: "scan_in_progress" });
    expect(screen.getByRole("alert")).toHaveTextContent(t("ui.scanning.busy"));
  });

  it("shows a generic message for other failures", async () => {
    const h = setup();
    render(<Scanning mode="quick" onDone={() => undefined} onCancel={() => undefined} />);
    await flush();
    await h.finish({ ok: false, error: "boom" });
    expect(screen.getByRole("alert")).toHaveTextContent(t("ui.scanning.failed"));
  });
});

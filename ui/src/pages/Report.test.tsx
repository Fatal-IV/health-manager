import { save } from "@tauri-apps/plugin-dialog";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api } from "../api";
import { t } from "../i18n";
import { report } from "../testing/fixtures";
import { Report } from "./Report";

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return { ...real, api: { exportReport: vi.fn() } };
});

const noop = () => undefined;

describe("Report", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists applied fixes and remaining issues", () => {
    render(
      <Report
        report={report()}
        applied={[{ fix: "empty_recycle_bin", outcome: "applied", at: "2026-10-24T09:31:00Z" }]}
        onBack={noop}
      />,
    );
    expect(screen.getByText(new RegExp(t("fix.empty_recycle_bin.summary")))).toBeInTheDocument();
    expect(screen.getByText(t("issue.storage.temp_files.title", { size_gb: "2.0" }))).toBeInTheDocument();
  });

  it("'why this score' shows each penalty", () => {
    render(<Report report={report()} applied={[]} onBack={noop} />);
    fireEvent.click(screen.getByText(t("ui.results.why_score")));
    expect(screen.getByText(new RegExp(t("ui.report.penalty", { points: 5 })))).toBeInTheDocument();
  });

  it("a scan without penalties says so", () => {
    render(<Report report={report({ score: { value: 100, breakdown: [] }, results: [] })} applied={[]} onBack={noop} />);
    fireEvent.click(screen.getByText(t("ui.results.why_score")));
    expect(screen.getByText(t("ui.report.no_penalties"))).toBeInTheDocument();
  });

  it("an incomplete scan shows no score and no score breakdown", () => {
    render(<Report report={report({ cancelled: true })} applied={[]} onBack={noop} />);
    expect(screen.getByText(t("ui.results.incomplete"))).toBeInTheDocument();
    expect(screen.queryByText(t("ui.results.why_score"))).not.toBeInTheDocument();
    expect(screen.queryByText(/\/100/)).not.toBeInTheDocument();
  });

  it("export buttons call the API with the format and the chosen path", async () => {
    vi.mocked(save).mockResolvedValueOnce("C:\\Users\\Mini Yönetim Uygulaması\\rapor.pdf");
    vi.mocked(api.exportReport).mockResolvedValue({ ok: true, value: undefined });
    render(<Report report={report()} applied={[]} onBack={noop} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.report.save_pdf") }));
    await waitFor(() =>
      expect(api.exportReport).toHaveBeenCalledWith("pdf", "C:\\Users\\Mini Yönetim Uygulaması\\rapor.pdf"),
    );
    expect(await screen.findByText(t("ui.report.saved"))).toBeInTheDocument();

    vi.mocked(save).mockResolvedValueOnce("C:\\r.txt");
    fireEvent.click(screen.getByRole("button", { name: t("ui.report.save_text") }));
    await waitFor(() => expect(api.exportReport).toHaveBeenLastCalledWith("text", "C:\\r.txt"));
  });

  it("closing the save dialog does nothing", async () => {
    vi.mocked(save).mockResolvedValueOnce(null);
    render(<Report report={report()} applied={[]} onBack={noop} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.report.save_text") }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(api.exportReport).not.toHaveBeenCalled();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("a failed export shows a friendly message", async () => {
    vi.mocked(save).mockResolvedValueOnce("C:\\r.txt");
    vi.mocked(api.exportReport).mockResolvedValue({ ok: false, error: "io: denied" });
    render(<Report report={report()} applied={[]} onBack={noop} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.report.save_text") }));
    expect(await screen.findByText(t("ui.report.save_failed"))).toBeInTheDocument();
    expect(screen.queryByText(/denied/)).not.toBeInTheDocument();
  });

  it("a dialog error is treated like a closed dialog", async () => {
    vi.mocked(save).mockRejectedValueOnce(new Error("no dialog"));
    render(<Report report={report()} applied={[]} onBack={noop} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.report.save_text") }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(api.exportReport).not.toHaveBeenCalled();
  });
});

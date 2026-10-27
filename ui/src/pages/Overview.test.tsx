import { fireEvent, render, screen } from "@testing-library/react";
import { api, type TroubleshooterId, type TroubleshootState } from "../api";
import { t } from "../i18n";
import { Overview } from "./Overview";

const st = (over: Partial<TroubleshootState> = {}): TroubleshootState => ({
  state: "healthy",
  step: null,
  total: 2,
  fix: null,
  reboot: false,
  unmeasured: false,
  hints: [],
  tested: true,
  ...over,
});
const ok = <T,>(value: T) => ({ ok: true as const, value });

function answers(map: Partial<Record<TroubleshooterId, ReturnType<typeof st> | null>>) {
  vi.spyOn(api, "troubleshootStart").mockImplementation(async (id) => {
    const v = map[id];
    return v ? ok(v) : { ok: false as const, error: "boom" };
  });
}

const row = (id: TroubleshooterId) => screen.getByTestId(`overview-${id}`);

describe("Overview", () => {
  afterEach(() => vi.restoreAllMocks());

  it("shows one row per troubleshooter with what its test found", async () => {
    answers({
      network: st(),
      windows_update: st({ state: "suggest", step: 0, fix: "clean_update_leftovers" }),
      system_files: st({ state: "suggest", step: 0, fix: "run_sfc_dism", tested: false }),
      slowness: null,
    });
    render(<Overview score={82} onOpen={() => undefined} onResults={() => undefined} onBack={() => undefined} />);
    await screen.findByText(t("ui.overview.status_healthy"));
    expect(row("network")).toHaveTextContent(t("ui.overview.status_healthy"));
    expect(row("windows_update")).toHaveTextContent(t("ui.overview.status_problem"));
    expect(row("system_files")).toHaveTextContent(t("ui.overview.status_notest"));
    expect(row("slowness")).toHaveTextContent(t("ui.overview.status_unmeasured"));
  });

  it("a test that could not run is never shown as healthy", async () => {
    answers({
      network: st({ state: "suggest", step: 0, fix: "flush_dns", unmeasured: true }),
      windows_update: st(),
      system_files: st({ tested: false, state: "suggest", step: 0, fix: "run_sfc_dism" }),
      slowness: st(),
    });
    render(<Overview score={null} onOpen={() => undefined} onResults={() => undefined} onBack={() => undefined} />);
    await screen.findAllByText(t("ui.overview.status_healthy"));
    expect(row("network")).toHaveTextContent(t("ui.overview.status_unmeasured"));
    expect(row("network")).not.toHaveTextContent(t("ui.overview.status_healthy"));
  });

  it("opens the chosen troubleshooter from its row", async () => {
    answers({ network: st({ state: "suggest", step: 0, fix: "flush_dns" }), windows_update: st(), system_files: st({ tested: false }), slowness: st() });
    const onOpen = vi.fn();
    render(<Overview score={90} onOpen={onOpen} onResults={() => undefined} onBack={() => undefined} />);
    await screen.findAllByText(t("ui.overview.status_healthy"));
    fireEvent.click(screen.getByRole("button", { name: t("ui.overview.fix") }));
    expect(onOpen).toHaveBeenCalledWith("network");
  });

  it("links to the scan results and shows the score", async () => {
    answers({ network: st(), windows_update: st(), system_files: st({ tested: false }), slowness: st() });
    const onResults = vi.fn();
    render(<Overview score={82} onOpen={() => undefined} onResults={onResults} onBack={() => undefined} />);
    expect(screen.getByText(t("ui.overview.score_line", { score: 82 }))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: t("ui.overview.see_results") }));
    expect(onResults).toHaveBeenCalled();
  });
});

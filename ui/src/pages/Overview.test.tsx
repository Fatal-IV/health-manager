import { fireEvent, render, screen, within } from "@testing-library/react";
import { api, TROUBLESHOOTERS, type TroubleshooterId, type TroubleshootState } from "../api";
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

function answerAll(pick: (id: TroubleshooterId) => TroubleshootState) {
  vi.spyOn(api, "troubleshootStart").mockImplementation(async (id) => ok(pick(id)));
}

const problem = () => st({ state: "suggest", step: 0, fix: "flush_dns" });
const show = (score: number | null = 82) =>
  render(<Overview score={score} onOpen={() => undefined} onResults={() => undefined} onBack={() => undefined} />);

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

  it("says it is checking while the tests are still running", () => {
    vi.spyOn(api, "troubleshootStart").mockImplementation(() => new Promise(() => undefined));
    show();
    expect(screen.getByText(t("ui.overview.summary_loading"))).toBeInTheDocument();
  });

  it("counts the areas that have a problem and marks their rows", async () => {
    answerAll((id) => (id === "network" || id === "slowness" ? problem() : st()));
    show();
    expect(await screen.findByText(t("ui.overview.summary_problems", { count: 2 }))).toBeInTheDocument();
    expect(row("network")).toHaveAttribute("data-state", "problem");
    expect(row("slowness")).toHaveAttribute("data-state", "problem");
    expect(row("windows_update")).toHaveAttribute("data-state", "healthy");
  });

  it("says everything is fine only when every test passed", async () => {
    answerAll(() => st());
    show();
    expect(await screen.findByText(t("ui.overview.summary_ok"))).toBeInTheDocument();
  });

  it("does not claim everything is fine when an area has no quick test", async () => {
    answerAll((id) => (id === "system_files" ? st({ state: "suggest", step: 0, fix: "run_sfc_dism", tested: false }) : st()));
    show();
    expect(await screen.findByText(t("ui.overview.summary_unchecked", { count: 1 }))).toBeInTheDocument();
    expect(screen.queryByText(t("ui.overview.summary_ok"))).not.toBeInTheDocument();
    expect(row("system_files")).toHaveAttribute("data-state", "notest");
  });

  it("describes each troubleshooter under its name", async () => {
    answerAll(() => st());
    show();
    await screen.findByText(t("ui.overview.summary_ok"));
    for (const id of TROUBLESHOOTERS) {
      expect(within(row(id)).getByText(t(`ui.troubleshoot.descriptions.${id}`))).toBeInTheDocument();
    }
  });

  it("has a row for every troubleshooter", async () => {
    vi.spyOn(api, "troubleshootStart").mockImplementation(async () => ok(st()));
    render(<Overview score={null} onOpen={() => undefined} onResults={() => undefined} onBack={() => undefined} />);
    await screen.findAllByText(t("ui.overview.status_healthy"));
    for (const id of TROUBLESHOOTERS) expect(row(id)).toBeInTheDocument();
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

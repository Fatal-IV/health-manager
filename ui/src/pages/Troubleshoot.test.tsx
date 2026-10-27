import { fireEvent, render, screen } from "@testing-library/react";
import { api, type TroubleshootState } from "../api";
import { t } from "../i18n";
import { applied, plan } from "../testing/fixtures";
import { Troubleshoot } from "./Troubleshoot";

const st = (over: Partial<TroubleshootState> = {}): TroubleshootState => ({
  state: "healthy",
  step: null,
  total: 3,
  fix: null,
  reboot: false,
  unmeasured: false,
  ...over,
});
const ok = <T,>(value: T) => ({ ok: true as const, value });
const suggest = (over: Partial<TroubleshootState> = {}) => st({ state: "suggest", step: 0, fix: "flush_dns", ...over });
const flushPlan = () => plan({ fix: "flush_dns", summary_key: "fix.flush_dns.summary", changes: ["fix.flush_dns.change.1"] });

async function applyFirstStep() {
  fireEvent.click(await screen.findByRole("button", { name: t("ui.troubleshoot.apply") }));
  fireEvent.click(await screen.findByRole("button", { name: t("ui.fix.confirm") }));
  fireEvent.click(await screen.findByRole("button", { name: t("ui.fix.done") }));
}

describe("Troubleshoot", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "getFixPlan").mockResolvedValue(ok(flushPlan()));
  });

  it("says nothing looks wrong when every test passes", async () => {
    vi.spyOn(api, "troubleshootStart").mockResolvedValue(ok(st()));
    render(<Troubleshoot onBack={() => undefined} />);
    expect(await screen.findByText(t("ui.troubleshoot.healthy_title"))).toBeInTheDocument();
  });

  it("proposes the first step, applies it through the normal preview, re-tests and resolves", async () => {
    vi.spyOn(api, "troubleshootStart").mockResolvedValue(ok(suggest()));
    const apply = vi.spyOn(api, "applyFix").mockResolvedValue(ok(applied()));
    const after = vi.spyOn(api, "troubleshootAfterFix").mockResolvedValue(ok(st({ state: "resolved" })));
    render(<Troubleshoot onBack={() => undefined} />);
    expect(await screen.findByText(t("ui.troubleshoot.step", { n: 1, total: 3 }))).toBeInTheDocument();
    await applyFirstStep();
    expect(apply).toHaveBeenCalledWith("flush_dns");
    expect(after).toHaveBeenCalledWith(0);
    expect(await screen.findByText(t("ui.troubleshoot.resolved_title"))).toBeInTheDocument();
  });

  it("a declined fix offers retry or skipping, and never runs the re-test", async () => {
    vi.spyOn(api, "troubleshootStart").mockResolvedValue(ok(suggest()));
    vi.spyOn(api, "applyFix").mockResolvedValue(ok({ ...applied(), outcome: { status: "declined" }, verified: false }));
    const after = vi.spyOn(api, "troubleshootAfterFix");
    const skip = vi.spyOn(api, "troubleshootSkip").mockResolvedValue(ok(suggest({ step: 1, fix: "renew_ip" })));
    render(<Troubleshoot onBack={() => undefined} />);
    await applyFirstStep();
    expect(await screen.findByRole("button", { name: t("ui.troubleshoot.retry") })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: t("ui.troubleshoot.skip") }));
    expect(await screen.findByText(t("ui.troubleshoot.step", { n: 2, total: 3 }))).toBeInTheDocument();
    expect(skip).toHaveBeenCalledWith(0);
    expect(after).not.toHaveBeenCalled();
  });

  it("when every step is used up it points to the report", async () => {
    vi.spyOn(api, "troubleshootStart").mockResolvedValue(ok(st({ state: "exhausted" })));
    render(<Troubleshoot onBack={() => undefined} />);
    expect(await screen.findByText(t("ui.troubleshoot.exhausted_title"))).toBeInTheDocument();
  });

  it("tells the user a restart is needed when the last fix asks for one", async () => {
    vi.spyOn(api, "troubleshootStart").mockResolvedValue(ok(st({ state: "resolved", reboot: true })));
    render(<Troubleshoot onBack={() => undefined} />);
    expect(await screen.findByText(t("ui.troubleshoot.reboot_note"))).toBeInTheDocument();
  });

  it("says a test could not be measured instead of calling it fine", async () => {
    vi.spyOn(api, "troubleshootStart").mockResolvedValue(ok(suggest({ unmeasured: true })));
    render(<Troubleshoot onBack={() => undefined} />);
    expect(await screen.findByText(t("ui.troubleshoot.unmeasured"))).toBeInTheDocument();
  });

  it("shows an error with a way back when the first test cannot start", async () => {
    vi.spyOn(api, "troubleshootStart").mockResolvedValue({ ok: false, error: "boom" });
    const onBack = vi.fn();
    render(<Troubleshoot onBack={onBack} />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: t("ui.troubleshoot.back") }));
    expect(onBack).toHaveBeenCalled();
  });
});

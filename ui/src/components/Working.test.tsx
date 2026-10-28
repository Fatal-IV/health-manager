import { act, render, screen } from "@testing-library/react";
import { api } from "../api";
import { t } from "../i18n";
import { plan } from "../testing/fixtures";
import { Working } from "./Working";

const twoOps = (over = {}) =>
  plan({ fix: "restart_spooler", needs_admin: false, ops: ["fix.restart_spooler.op.1", "fix.restart_spooler.op.2"], ...over });

const states = () => screen.getAllByRole("listitem").map((li) => li.getAttribute("data-state"));
const value = () => Number(screen.getByRole("progressbar").getAttribute("aria-valuenow"));

let emit: (p: { done: number; total: number }) => void = () => undefined;

beforeEach(() => {
  vi.restoreAllMocks();
  emit = () => undefined;
  vi.spyOn(api, "onFixProgress").mockImplementation((cb) => {
    emit = cb;
    return Promise.resolve(() => undefined);
  });
});

describe("Working", () => {
  it("shows the title, the step and the operations", () => {
    render(<Working title="Yazıcı" step={1} total={4} plan={twoOps()} stage="applying" mode="live" />);
    expect(screen.getByRole("heading", { name: "Yazıcı" })).toBeInTheDocument();
    expect(screen.getByText(t("ui.troubleshoot.step", { n: 2, total: 4 }))).toBeInTheDocument();
    expect(screen.getByText(t("fix.restart_spooler.op.1"))).toBeInTheDocument();
    expect(screen.getByText(t("ui.working.applying"))).toBeInTheDocument();
  });

  it("live mode marks operations as events arrive", async () => {
    render(<Working title="x" step={0} total={1} plan={twoOps()} stage="applying" mode="live" />);
    expect(states()).toEqual(["current", "pending"]);
    expect(value()).toBe(0);
    await act(async () => emit({ done: 1, total: 2 }));
    expect(states()).toEqual(["done", "current"]);
    expect(value()).toBe(33);
  });

  it("live mode without events keeps the first operation current and moves with the stage", () => {
    const { rerender } = render(<Working title="x" step={0} total={1} plan={twoOps()} stage="applying" mode="live" />);
    expect(states()).toEqual(["current", "pending"]);
    rerender(<Working title="x" step={0} total={1} plan={twoOps()} stage="checking" mode="live" />);
    expect(states()).toEqual(["done", "done"]);
    expect(value()).toBe(66);
    expect(screen.getByText(t("ui.working.checking"))).toBeInTheDocument();
  });

  it("staged mode eases toward but never reaches the applying target", () => {
    vi.useFakeTimers();
    try {
      render(<Working title="x" step={0} total={1} plan={twoOps()} stage="applying" mode="staged" />);
      expect(value()).toBe(0);
      act(() => void vi.advanceTimersByTime(1000));
      expect(value()).toBeGreaterThan(0);
      act(() => void vi.advanceTimersByTime(10_000));
      expect(value()).toBeLessThan(67);
      expect(states()).toEqual(["current", "pending"]);
      expect(api.onFixProgress).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("staged checking stage moves toward complete", () => {
    vi.useFakeTimers();
    try {
      render(<Working title="x" step={0} total={1} plan={twoOps()} stage="checking" mode="staged" />);
      expect(value()).toBeGreaterThanOrEqual(66);
      act(() => void vi.advanceTimersByTime(5000));
      expect(value()).toBeGreaterThan(66);
      expect(value()).toBeLessThan(100);
      expect(states()).toEqual(["done", "done"]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows the admin note only for admin fixes", () => {
    const { rerender } = render(<Working title="x" step={0} total={1} plan={twoOps()} stage="applying" mode="live" />);
    expect(screen.queryByText(t("ui.working.admin_note"))).not.toBeInTheDocument();
    rerender(<Working title="x" step={0} total={1} plan={twoOps({ needs_admin: true })} stage="applying" mode="live" />);
    expect(screen.getByText(t("ui.working.admin_note"))).toBeInTheDocument();
  });
});

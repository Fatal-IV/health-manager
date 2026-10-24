import { fireEvent, render, screen } from "@testing-library/react";
import { t } from "../i18n";
import { applied, plan } from "../testing/fixtures";
import { FixPreview } from "./FixPreview";

const noop = () => undefined;

function show(over: Partial<React.ComponentProps<typeof FixPreview>> = {}) {
  return render(
    <FixPreview plan={plan()} busy={false} result={null} onConfirm={noop} onCancel={noop} onDone={noop} {...over} />,
  );
}

describe("FixPreview", () => {
  it("lists what will change before asking for confirmation", () => {
    const onConfirm = vi.fn();
    show({ onConfirm });
    expect(screen.getByText(t("fix.clean_temp.summary"))).toBeInTheDocument();
    expect(screen.getByText(t("fix.clean_temp.change.1"))).toBeInTheDocument();
    expect(screen.getByText(t("fix.clean_temp.change.2"))).toBeInTheDocument();
    expect(onConfirm).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: t("ui.fix.confirm") }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("warns that Windows will ask for permission only when admin is needed", () => {
    const { unmount } = show();
    expect(screen.queryByText(t("ui.fix.admin_note"))).not.toBeInTheDocument();
    unmount();
    show({ plan: plan({ needs_admin: true }) });
    expect(screen.getByText(t("ui.fix.admin_note"))).toBeInTheDocument();
  });

  it("says whether the change can be undone", () => {
    const { unmount } = show({ plan: plan({ reversible: true }) });
    expect(screen.getByText(t("ui.fix.reversible"))).toBeInTheDocument();
    unmount();
    show({ plan: plan({ reversible: false }) });
    expect(screen.getByText(t("ui.fix.not_reversible"))).toBeInTheDocument();
  });

  it("fills the target name into the text", () => {
    show({
      plan: plan({
        fix: "disable_startup_item",
        summary_key: "fix.disable_startup_item.summary",
        changes: ["fix.disable_startup_item.change.1"],
        params: { target: "Updater" },
      }),
    });
    expect(screen.getByText(/Updater/)).toBeInTheDocument();
  });

  it("locks the buttons while the fix runs so it cannot be started twice", () => {
    show({ busy: true });
    expect(screen.getByRole("button", { name: t("ui.fix.working") })).toBeDisabled();
    expect(screen.getByRole("button", { name: t("ui.fix.cancel") })).toBeDisabled();
  });

  it("a declined UAC prompt says nothing was changed", () => {
    show({ result: applied({ outcome: { status: "declined" }, verified: false }) });
    expect(screen.getByRole("status")).toHaveTextContent(t("fix.result.declined"));
    expect(screen.queryByRole("button", { name: t("ui.fix.confirm") })).not.toBeInTheDocument();
  });

  it("a failed fix hides the technical detail until it is expanded", () => {
    show({
      result: applied({
        outcome: { status: "failed", message: "fix.result.failed", params: {}, detail: "helper exit 3\nsecret-path" },
        verified: false,
      }),
    });
    expect(screen.getByRole("alert")).toHaveTextContent(t("fix.result.failed"));
    const details = screen.getByText(/secret-path/).closest("details")!;
    expect(details.open).toBe(false);
    fireEvent.click(screen.getByText(t("ui.fix.technical")));
    expect(details.open).toBe(true);
  });

  it("an applied but unverified fix says the problem may still be there", () => {
    show({ result: applied({ verified: false }) });
    expect(screen.getByText(t("ui.fix.unverified"))).toBeInTheDocument();
    expect(screen.queryByText(t("ui.fix.verified"))).not.toBeInTheDocument();
  });

  it("an applied and verified fix says so", () => {
    show({ result: applied({ verified: true }) });
    expect(screen.getByText(t("ui.fix.verified"))).toBeInTheDocument();
  });

  it("the done button leaves the outcome screen", () => {
    const onDone = vi.fn();
    show({ result: applied(), onDone });
    fireEvent.click(screen.getByRole("button", { name: t("ui.fix.done") }));
    expect(onDone).toHaveBeenCalled();
  });
});

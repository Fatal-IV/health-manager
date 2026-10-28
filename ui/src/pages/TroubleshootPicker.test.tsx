import { fireEvent, render, screen } from "@testing-library/react";
import { TROUBLESHOOTERS } from "../api";
import { t } from "../i18n";
import { TroubleshootPicker } from "./TroubleshootPicker";

describe("TroubleshootPicker", () => {
  it("offers every troubleshooter and opens the chosen one", () => {
    const onPick = vi.fn();
    render(<TroubleshootPicker onPick={onPick} onBack={() => undefined} />);
    for (const id of TROUBLESHOOTERS) {
      expect(screen.getByRole("button", { name: new RegExp(t(`ui.troubleshoot.titles.${id}`)) })).toBeInTheDocument();
    }
    const last = TROUBLESHOOTERS[TROUBLESHOOTERS.length - 1];
    fireEvent.click(screen.getByRole("button", { name: new RegExp(t(`ui.troubleshoot.titles.${last}`)) }));
    expect(onPick).toHaveBeenCalledWith(last);
  });

  it("goes back", () => {
    const onBack = vi.fn();
    render(<TroubleshootPicker onPick={() => undefined} onBack={onBack} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.troubleshoot.back") }));
    expect(onBack).toHaveBeenCalled();
  });
});

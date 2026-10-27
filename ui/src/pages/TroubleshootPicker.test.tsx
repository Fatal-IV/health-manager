import { fireEvent, render, screen } from "@testing-library/react";
import { t } from "../i18n";
import { TroubleshootPicker } from "./TroubleshootPicker";

describe("TroubleshootPicker", () => {
  it("offers all four troubleshooters and opens the chosen one", () => {
    const onPick = vi.fn();
    render(<TroubleshootPicker onPick={onPick} onBack={() => undefined} />);
    for (const id of ["network", "windows_update", "system_files", "slowness"]) {
      expect(screen.getByRole("button", { name: new RegExp(t(`ui.troubleshoot.titles.${id}`)) })).toBeInTheDocument();
    }
    fireEvent.click(screen.getByRole("button", { name: new RegExp(t("ui.troubleshoot.titles.slowness")) }));
    expect(onPick).toHaveBeenCalledWith("slowness");
  });

  it("goes back", () => {
    const onBack = vi.fn();
    render(<TroubleshootPicker onPick={() => undefined} onBack={onBack} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.troubleshoot.back") }));
    expect(onBack).toHaveBeenCalled();
  });
});

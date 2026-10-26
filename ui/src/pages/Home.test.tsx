import { fireEvent, render, screen } from "@testing-library/react";
import { t } from "../i18n";
import { Home } from "./Home";

describe("Home", () => {
  beforeEach(() => localStorage.clear());

  it("shows the first-run welcome only once", () => {
    const first = render(<Home onStart={() => undefined} />);
    expect(screen.getByText(t("ui.home.welcome_body"))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.welcome_ok") }));
    expect(screen.queryByText(t("ui.home.welcome_body"))).not.toBeInTheDocument();
    first.unmount();

    render(<Home onStart={() => undefined} />);
    expect(screen.queryByText(t("ui.home.welcome_body"))).not.toBeInTheDocument();
  });

  it("still renders when storage is unavailable", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    render(<Home onStart={() => undefined} />);
    expect(screen.getByText(t("ui.home.welcome_body"))).toBeInTheDocument();
    spy.mockRestore();
  });

  it("start button asks for a quick scan", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    expect(onStart).toHaveBeenCalledWith("quick");
  });

  it("deep scan option asks for a deep scan", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    fireEvent.click(screen.getByLabelText(t("ui.home.deep")));
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    expect(onStart).toHaveBeenCalledWith("deep");
  });

  it("a subset of areas is passed on, in a stable order", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    fireEvent.click(screen.getByLabelText(t("category.update")));
    fireEvent.click(screen.getByLabelText(t("category.system")));
    fireEvent.click(screen.getByRole("button", { name: t("ui.home.start") }));
    expect(onStart).toHaveBeenCalledWith("quick", ["security", "storage", "performance"]);
  });

  it("with no area selected the scan cannot start", () => {
    const onStart = vi.fn();
    render(<Home onStart={onStart} />);
    for (const c of ["security", "update", "storage", "performance", "system"]) fireEvent.click(screen.getByLabelText(t(`category.${c}`)));
    expect(screen.getByRole("button", { name: t("ui.home.start") })).toBeDisabled();
    expect(screen.getByText(t("ui.home.pick_one"))).toBeInTheDocument();
  });

  it("shows no score and 'never scanned' before the first scan, the score after", () => {
    const { rerender } = render(<Home onStart={() => undefined} />);
    expect(screen.getByRole("img")).toHaveAttribute("aria-label", `${t("ui.results.score")}: —`);
    expect(screen.getByText(t("ui.home.never"))).toBeInTheDocument();
    rerender(<Home onStart={() => undefined} score={87} lastScan="2026-10-24T09:30:00Z" />);
    expect(screen.getByRole("img")).toHaveAttribute("aria-label", `${t("ui.results.score")}: 87`);
    expect(screen.queryByText(t("ui.home.never"))).not.toBeInTheDocument();
  });
});

import { act, fireEvent, render, screen } from "@testing-library/react";
import { api } from "../api";
import { t } from "../i18n";
import { noteLines, UpdateCorner } from "./UpdateCorner";

const SEEN = "saglik-denetimi.seen-version";
const info = { version: "9.9.9", notes: "" };
const flush = () => act(async () => undefined);
const panel = () => screen.getByRole("status", { hidden: true });
const clickPill = () => fireEvent.click(screen.getByRole("button", { name: t("ui.update.corner") }));

describe("UpdateCorner", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.spyOn(api, "getReleaseNotes").mockResolvedValue({ ok: true, value: null });
  });
  afterEach(() => vi.restoreAllMocks());

  it("is a closed corner box until clicked, and does not check by itself", async () => {
    const check = vi.spyOn(api, "checkUpdate");
    render(<UpdateCorner />);
    await flush();
    expect(screen.getByRole("button", { name: t("ui.update.corner") })).toBeInTheDocument();
    expect(panel()).toHaveAttribute("data-open", "false");
    expect(check).not.toHaveBeenCalled();
  });

  it("clicking opens the box and checks at once; no newer release says so", async () => {
    vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: true, value: null });
    render(<UpdateCorner />);
    clickPill();
    expect(panel()).toHaveAttribute("data-open", "true");
    expect(panel()).toHaveTextContent(t("ui.update.checking_title"));
    await flush();
    expect(panel()).toHaveTextContent(t("ui.update.current_title"));
  });

  it("a newer release is downloaded and installed without another click", async () => {
    vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: true, value: info });
    const install = vi.spyOn(api, "installUpdate").mockImplementation(() => new Promise(() => undefined));
    render(<UpdateCorner />);
    clickPill();
    await flush();
    expect(install).toHaveBeenCalledTimes(1);
    expect(panel()).toHaveTextContent("9.9.9");
    expect(screen.getByRole("button", { name: t("ui.update.close"), hidden: true })).toBeDisabled();
  });

  it("a failed check or install says so and can be retried", async () => {
    const check = vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: false, error: "offline" });
    render(<UpdateCorner />);
    clickPill();
    await flush();
    expect(panel()).toHaveTextContent(t("ui.update.failed"));

    check.mockResolvedValue({ ok: true, value: info });
    vi.spyOn(api, "installUpdate").mockResolvedValue({ ok: false, error: "signature does not match" });
    fireEvent.click(screen.getByRole("button", { name: t("ui.update.retry") }));
    await flush();
    expect(check).toHaveBeenCalledTimes(2);
    expect(panel()).toHaveTextContent(t("ui.update.failed"));
    expect(screen.getByRole("button", { name: t("ui.update.retry") })).toBeEnabled();
  });

  it("after an update the box opens by itself with the notes, once", async () => {
    localStorage.setItem(SEEN, "0.0.1");
    vi.spyOn(api, "getReleaseNotes").mockResolvedValue({ ok: true, value: { version: __APP_VERSION__, notes: "- Yeni kutu\n- Hata düzeltildi" } });
    render(<UpdateCorner />);
    await flush();
    expect(panel()).toHaveAttribute("data-open", "true");
    expect(panel()).toHaveTextContent(t("ui.update.news_title", { version: __APP_VERSION__ }));
    expect(screen.getByText("Yeni kutu")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: t("ui.update.news_ok") }));
    expect(panel()).toHaveAttribute("data-open", "false");
    expect(localStorage.getItem(SEEN)).toBe(__APP_VERSION__);
  });

  it("the very first start shows no news, and an unreachable network keeps them for later", async () => {
    vi.spyOn(api, "getReleaseNotes").mockResolvedValue({ ok: true, value: { version: __APP_VERSION__, notes: "- x" } });
    const first = render(<UpdateCorner />);
    await flush();
    expect(panel()).toHaveAttribute("data-open", "false");
    expect(localStorage.getItem(SEEN)).toBe(__APP_VERSION__);
    first.unmount();

    localStorage.setItem(SEEN, "0.0.1");
    vi.spyOn(api, "getReleaseNotes").mockResolvedValue({ ok: false, error: "offline" });
    render(<UpdateCorner />);
    await flush();
    expect(panel()).toHaveAttribute("data-open", "false");
    expect(localStorage.getItem(SEEN)).toBe("0.0.1");
  });

  it("notes become one item per line", () => {
    expect(noteLines("- bir\n* iki\n\n  üç  ")).toEqual(["bir", "iki", "üç"]);
  });
});

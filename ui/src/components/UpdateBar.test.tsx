import { act, fireEvent, render, screen } from "@testing-library/react";
import { api } from "../api";
import { t } from "../i18n";
import { UpdateBar } from "./UpdateBar";

const info = { version: "0.3.0", notes: "" };
const flush = () => act(async () => undefined);

describe("UpdateBar", () => {
  afterEach(() => vi.restoreAllMocks());

  it("shows nothing when there is no update or the check failed", async () => {
    vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: true, value: null });
    const first = render(<UpdateBar />);
    await flush();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    first.unmount();

    vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: false, error: "offline" });
    render(<UpdateBar />);
    await flush();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("offers the new version and installs on click", async () => {
    vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: true, value: info });
    const install = vi.spyOn(api, "installUpdate").mockResolvedValue({ ok: true, value: undefined });
    render(<UpdateBar />);
    await flush();
    expect(screen.getByRole("status")).toHaveTextContent("0.3.0");
    fireEvent.click(screen.getByRole("button", { name: t("ui.update.install") }));
    expect(install).toHaveBeenCalledTimes(1);
  });

  it("a failed install says so and allows another try", async () => {
    vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: true, value: info });
    vi.spyOn(api, "installUpdate").mockResolvedValue({ ok: false, error: "signature does not match" });
    render(<UpdateBar />);
    await flush();
    fireEvent.click(screen.getByRole("button", { name: t("ui.update.install") }));
    await flush();
    expect(screen.getByRole("status")).toHaveTextContent(t("ui.update.failed"));
    expect(screen.getByRole("button", { name: t("ui.update.retry") })).toBeEnabled();
  });

  it("can be dismissed", async () => {
    vi.spyOn(api, "checkUpdate").mockResolvedValue({ ok: true, value: info });
    render(<UpdateBar />);
    await flush();
    fireEvent.click(screen.getByRole("button", { name: t("ui.update.later") }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

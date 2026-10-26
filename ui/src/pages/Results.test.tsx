import { act, fireEvent, render, screen } from "@testing-library/react";
import { api } from "../api";
import { categoryStatus } from "../components/CategoryCard";
import { parseStartupItems } from "../components/IssueItem";
import { t } from "../i18n";
import { check, issue, report } from "../testing/fixtures";
import { Results } from "./Results";

const noop = () => undefined;

describe("Results", () => {
  it("card status follows the worst issue", () => {
    const r = report({
      results: [
        check({ issues: [issue({ status: "warning" }), issue({ id: "b", status: "critical" })] }),
        check({ check_id: "u", category: "update", issues: [] }),
      ],
    });
    render(<Results report={r} onFix={noop} onReport={noop} onRescan={noop} />);
    expect(screen.getByTestId("card-storage")).toHaveAttribute("data-status", "critical");
    expect(screen.getByTestId("card-update")).toHaveAttribute("data-status", "good");
  });

  it("a check that could not run is unknown, never good", () => {
    const failed = check({ check_id: "system.boot", category: "system", issues: [], error: "access denied" });
    expect(categoryStatus([failed])).toBe("unknown");
    render(<Results report={report({ results: [failed] })} onFix={noop} onReport={noop} onRescan={noop} />);
    const card = screen.getByTestId("card-system");
    expect(card).toHaveAttribute("data-status", "unknown");
    fireEvent.click(card);
    expect(screen.getByText(t("ui.results.could_not_check"))).toBeInTheDocument();
  });

  it("an unknown issue is muted and offers no fix", () => {
    const r = report({ results: [check({ issues: [issue({ status: "unknown", fix: null })] })] });
    render(<Results report={r} onFix={noop} onReport={noop} onRescan={noop} />);
    fireEvent.click(screen.getByTestId("card-storage"));
    expect(screen.queryByRole("button", { name: t("ui.results.fix") })).not.toBeInTheDocument();
  });

  it("offers a fix button only for issues that have a fix", () => {
    const r = report({
      results: [
        check({ issues: [issue({ id: "a", fix: "clean_temp" }), issue({ id: "b", fix: null })] }),
      ],
    });
    render(<Results report={r} onFix={noop} onReport={noop} onRescan={noop} />);
    expect(screen.getAllByRole("button", { name: t("ui.results.fix") })).toHaveLength(1);
  });

  it("has no bulk 'fix everything' button: every fix is confirmed one by one", () => {
    const r = report({
      results: [check({ issues: [issue({ id: "a" }), issue({ id: "b", fix: "empty_recycle_bin" })] })],
    });
    render(<Results report={r} onFix={noop} onReport={noop} onRescan={noop} />);
    const fixButtons = screen.getAllByRole("button", { name: t("ui.results.fix") });
    expect(fixButtons).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /tümü|hepsi|toplu/i })).not.toBeInTheDocument();
  });

  it("clicking a fix passes the issue to the handler", () => {
    const onFix = vi.fn();
    render(<Results report={report()} onFix={onFix} onReport={noop} onRescan={noop} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.results.fix") }));
    expect(onFix).toHaveBeenCalledTimes(1);
    expect(onFix.mock.calls[0][0].id).toBe("storage.temp_files");
  });

  it("lists startup items with one button each, passing the item id and name", () => {
    const items = JSON.stringify([
      { id: "HKCU|Foo/Bar", name: "Foo" },
      { id: "HKLM|Vendor\\Updater", name: "Updater" },
    ]);
    const startup = issue({
      id: "performance.startup_items",
      category: "performance",
      title_key: "issue.performance.startup_items.title",
      why_key: "issue.performance.startup_items.why",
      params: { count: "12", items },
      fix: "disable_startup_item",
    });
    const onFix = vi.fn();
    render(
      <Results
        report={report({ results: [check({ check_id: "p", category: "performance", issues: [startup] })] })}
        onFix={onFix}
        onReport={noop}
        onRescan={noop}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: `${t("ui.results.fix_item")}: Updater` }));
    expect(onFix).toHaveBeenCalledWith(startup, "HKLM|Vendor\\Updater", "Updater");
    expect(screen.getAllByRole("button", { name: new RegExp(t("ui.results.fix_item")) })).toHaveLength(2);
  });

  it("update issues offer to open Windows Update", () => {
    const open = vi.spyOn(api, "openWindowsUpdate").mockResolvedValue({ ok: true, value: undefined });
    const pending = issue({ id: "update.pending", category: "update", fix: null });
    render(
      <Results report={report({ results: [check({ check_id: "u", category: "update", issues: [pending] })] })} onFix={noop} onReport={noop} onRescan={noop} />,
    );
    fireEvent.click(screen.getByRole("button", { name: t("ui.results.open_update") }));
    expect(open).toHaveBeenCalledTimes(1);
  });

  it("picked fixes go to the batch handler together; startup items cannot be picked", () => {
    const a = issue({ id: "a", fix: "clean_temp" });
    const b = issue({ id: "b", fix: "empty_recycle_bin" });
    const startup = issue({ id: "performance.startup_items", category: "performance", fix: "disable_startup_item", params: { items: "[]" } });
    const onBulk = vi.fn();
    render(
      <Results
        report={report({ results: [check({ issues: [a, b, startup] })] })}
        onFix={noop}
        onReport={noop}
        onRescan={noop}
        onBulkFix={onBulk}
      />,
    );
    expect(screen.queryByRole("button", { name: new RegExp(t("ui.results.fix_selected")) })).not.toBeInTheDocument();
    const boxes = screen.getAllByRole("checkbox");
    expect(boxes).toHaveLength(2);
    fireEvent.click(boxes[0]);
    fireEvent.click(boxes[1]);
    fireEvent.click(screen.getByRole("button", { name: `${t("ui.results.fix_selected")} (2)` }));
    expect(onBulk).toHaveBeenCalledTimes(1);
    expect(onBulk.mock.calls[0][0].map((i: { id: string }) => i.id)).toEqual(["a", "b"]);
  });

  it("re-scanning one area reports that area", () => {
    const onArea = vi.fn();
    render(<Results report={report()} onFix={noop} onReport={noop} onRescan={noop} onRescanArea={onArea} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.results.rescan_area") }));
    expect(onArea).toHaveBeenCalledWith("storage");
  });

  it("an unreadable boot time offers a one-click read through the normal fix flow", () => {
    const boot = issue({ id: "system.boot_unknown", category: "system", status: "unknown", fix: "read_boot_time", title_key: "issue.system.boot_unknown.title", why_key: "issue.system.boot_unknown.why" });
    const onFix = vi.fn();
    render(<Results report={report({ results: [check({ check_id: "system.boot", category: "system", issues: [boot] })] })} onFix={onFix} onReport={noop} onRescan={noop} />);
    fireEvent.click(screen.getByRole("button", { name: t("ui.results.read_boot_time") }));
    expect(onFix).toHaveBeenCalledWith(boot);
  });

  it("parseStartupItems ignores garbage", () => {
    expect(parseStartupItems(undefined)).toEqual([]);
    expect(parseStartupItems("not json")).toEqual([]);
    expect(parseStartupItems('{"a":1}')).toEqual([]);
    expect(parseStartupItems('[{"id":1},{"id":"x","name":"y"}]')).toEqual([{ id: "x", name: "y" }]);
  });

  it("an incomplete scan shows no score and says so", () => {
    render(<Results report={report({ cancelled: true })} onFix={noop} onReport={noop} onRescan={noop} />);
    expect(screen.getByRole("img")).toHaveAttribute("aria-label", `${t("ui.results.score")}: —`);
    expect(screen.getByText(t("ui.results.incomplete"))).toBeInTheDocument();
  });

  it("technical evidence stays inside a collapsed details element", () => {
    render(<Results report={report()} onFix={noop} onReport={noop} onRescan={noop} />);
    const details = screen.getByText("temp 2.0 GiB").closest("details")!;
    expect(details.open).toBe(false);
  });
});

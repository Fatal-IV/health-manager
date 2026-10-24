import { fireEvent, render, screen, within } from "@testing-library/react";
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
    fireEvent.click(within(card).getByRole("button"));
    expect(within(card).getByText(t("ui.results.could_not_check"))).toBeInTheDocument();
  });

  it("an unknown issue is muted and offers no fix", () => {
    const r = report({ results: [check({ issues: [issue({ status: "unknown", fix: null })] })] });
    render(<Results report={r} onFix={noop} onReport={noop} onRescan={noop} />);
    fireEvent.click(within(screen.getByTestId("card-storage")).getByRole("button", { name: /.*/ }));
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

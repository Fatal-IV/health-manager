import tr from "./tr.json";
import { t } from "./index";

const sources = import.meta.glob(["../**/*.ts", "../**/*.tsx", "!../**/*.test.*", "!../testing/**"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function lookup(key: string): unknown {
  return key.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown> | undefined)?.[k], tr);
}

describe("i18n", () => {
  it("every literal t() key used in src exists in tr.json", () => {
    const missing: string[] = [];
    let found = 0;
    for (const [file, text] of Object.entries(sources)) {
      for (const m of text.matchAll(/\bt\(\s*"([a-z0-9_.]+)"/g)) {
        found++;
        if (typeof lookup(m[1]) !== "string") missing.push(`${file}: ${m[1]}`);
      }
    }
    expect(found).toBeGreaterThan(30);
    expect(missing).toEqual([]);
  });

  it("dynamic key families exist for every value the UI can build", () => {
    for (const c of ["storage", "performance", "update", "security", "system"]) {
      expect(typeof lookup(`category.${c}`)).toBe("string");
    }
    for (const s of ["good", "warning", "critical", "unknown"]) {
      expect(typeof lookup(`status.${s}`)).toBe("string");
    }
    for (const f of [
      "clean_temp",
      "empty_recycle_bin",
      "clean_update_leftovers",
      "disable_startup_item",
      "end_process",
      "reset_windows_update",
      "enable_defender_realtime",
      "enable_firewall",
      "run_sfc_dism",
    ]) {
      expect(typeof lookup(`fix.${f}.summary`)).toBe("string");
    }
    for (const o of ["applied", "unverified"]) {
      expect(typeof lookup(`report.outcome.${o}`)).toBe("string");
    }
  });

  it("interpolates params into issue text", () => {
    expect(t("issue.storage.temp_files.title", { size_gb: "2.0" })).toContain("2.0");
  });

  it("a missing key is shown as the key, not blank", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    expect(t("no.such.key")).toBe("no.such.key");
    warn.mockRestore();
  });

  it("a numeric-looking count param does not break interpolation", () => {
    expect(t("issue.performance.startup_items.title", { count: "12" })).not.toMatch(/\{\{/);
    expect(t("issue.performance.startup_items.why", { count: "12" })).toContain("12");
  });
});

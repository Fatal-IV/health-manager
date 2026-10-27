import { formatBoot, formatGb, formatUptime, shortOsName } from "./format";

const GIB = 1024 ** 3;

describe("format", () => {
  it("gigabytes use a decimal comma below 100 and whole numbers above", () => {
    expect(formatGb(9.3 * GIB)).toBe("9,3");
    expect(formatGb(142 * GIB)).toBe("142");
    expect(formatGb(0)).toBe("0,0");
  });

  it("uptime shows the largest whole unit", () => {
    expect(formatUptime(3 * 86400 + 5)).toBe("3 gün");
    expect(formatUptime(5 * 3600 + 90)).toBe("5 saat");
    expect(formatUptime(42 * 60)).toBe("42 dakika");
    expect(formatUptime(10)).toBe("1 dakika");
  });

  it("the system name is shortened to the part a person recognises", () => {
    expect(shortOsName("Windows 11 Home Single Language")).toBe("11 Home");
    expect(shortOsName("Microsoft Windows 10 Pro")).toBe("10 Pro");
    expect(shortOsName("Windows 11")).toBe("11");
  });
});

describe("formatBoot", () => {
  it("writes seconds, then minutes and seconds", () => {
    expect(formatBoot(0.2)).toBe("1 sn");
    expect(formatBoot(48)).toBe("48 sn");
    expect(formatBoot(65)).toBe("1 dk 5 sn");
    expect(formatBoot(180)).toBe("3 dk");
  });
});

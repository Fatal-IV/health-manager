const GIB = 1024 ** 3;

/** Gigabytes the way a Turkish reader writes them: `142`, `9,3`. */
export function formatGb(bytes: number): string {
  const gb = bytes / GIB;
  return gb >= 100 ? String(Math.round(gb)) : gb.toFixed(1).replace(".", ",");
}

/** The largest whole unit: `3 gün`, `5 saat`, `42 dakika` (never less than a minute). */
export function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86400);
  if (days >= 1) return `${days} gün`;
  const hours = Math.floor(secs / 3600);
  if (hours >= 1) return `${hours} saat`;
  return `${Math.max(1, Math.floor(secs / 60))} dakika`;
}

/** `Windows 11 Home Single Language` -> `11 Home`. */
export function shortOsName(os: string): string {
  return os.replace(/^Microsoft\s+/i, "").replace(/^Windows\s+/i, "").split(/\s+/).slice(0, 2).join(" ");
}

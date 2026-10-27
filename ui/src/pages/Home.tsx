import { CheckCircle, Play, Wrench } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { api, type Category, type ScanMode, type SystemSummary } from "../api";
import { formatBoot, formatGb, formatUptime, shortOsName } from "../format";
import { t } from "../i18n";

const AREAS: Category[] = ["security", "update", "storage", "performance", "system"];

interface Tile {
  key: string;
  label: string;
  value: string;
  sub?: string;
  /** 0-100: draws a thin bar under the value. */
  fill?: number;
}

/** One tile per fact that could be read; a fact the system would not give is simply absent. */
export function summaryTiles(s: SystemSummary): Tile[] {
  const tiles: Tile[] = [];
  if (s.os) tiles.push({ key: "os", label: t("ui.home.tile_windows"), value: shortOsName(s.os) });
  if (s.uptime_secs !== null) {
    tiles.push({ key: "uptime", label: t("ui.home.tile_uptime"), value: formatUptime(s.uptime_secs), sub: t("ui.home.tile_uptime_sub") });
  }
  if (s.boot_secs !== null && s.boot_secs !== undefined) {
    tiles.push({ key: "boot", label: t("ui.home.tile_boot"), value: formatBoot(s.boot_secs), sub: t("ui.home.tile_boot_sub") });
  }
  if (s.disk) {
    const used = s.disk.total_bytes - s.disk.free_bytes;
    tiles.push({
      key: "disk",
      label: t("ui.home.tile_disk", { mount: s.disk.mount }),
      value: `${formatGb(s.disk.free_bytes)} GB`,
      sub: t("ui.home.tile_disk_sub", { total: formatGb(s.disk.total_bytes), free: formatGb(s.disk.free_bytes) }),
      fill: Math.round((used / s.disk.total_bytes) * 100),
    });
  }
  if (s.memory) {
    const pct = Math.round((s.memory.used_bytes / s.memory.total_bytes) * 100);
    tiles.push({
      key: "memory",
      label: t("ui.home.tile_memory"),
      value: `%${pct}`,
      sub: t("ui.home.tile_memory_sub", { total: formatGb(s.memory.total_bytes), used: formatGb(s.memory.used_bytes) }),
      fill: pct,
    });
  }
  return tiles;
}

export function Home({
  onStart,
  onTroubleshoot,
  lastScan,
  score,
}: {
  onStart: (mode: ScanMode, categories?: Category[]) => void;
  onTroubleshoot?: () => void;
  lastScan?: string;
  score?: number | null;
}) {
  const [summary, setSummary] = useState<SystemSummary | null>(null);
  const [editing, setEditing] = useState(false);
  const [deep, setDeep] = useState(false);
  const [areas, setAreas] = useState<Category[]>(AREAS);

  useEffect(() => {
    let alive = true;
    void api.getSystemSummary().then((r) => alive && r.ok && setSummary(r.value));
    return () => {
      alive = false;
    };
  }, []);

  const toggleArea = (c: Category) => setAreas((a) => (a.includes(c) ? a.filter((x) => x !== c) : [...a, c]));
  const start = () => {
    const mode: ScanMode = deep ? "deep" : "quick";
    // Every area selected is the plain full scan.
    if (areas.length === AREAS.length) onStart(mode);
    else onStart(mode, AREAS.filter((c) => areas.includes(c)));
  };

  const tiles = summary ? summaryTiles(summary) : [];
  const last = lastScan ? t("ui.home.last_scan", { date: new Date(lastScan).toLocaleString("tr-TR") }) : t("ui.home.never");
  const hasScore = score !== null && score !== undefined;

  return (
    <main className="home-b">
      <p className="eyebrow">{t("ui.home.eyebrow")}</p>
      <h1>{t("ui.home.headline")}</h1>
      <p className="lead">{t("ui.home.lead")}</p>
      <p className="trust">
        <CheckCircle size={18} weight="fill" aria-hidden />
        {t("ui.home.trust")}
      </p>

      {tiles.length > 0 && (
        <ul className="tiles" aria-label={t("ui.home.summary")}>
          {tiles.map((x) => (
            <li key={x.key} className="tile">
              <span className="tile-label">{x.label}</span>
              <span className="tile-value">{x.value}</span>
              {x.fill !== undefined && (
                <span className="tile-bar" aria-hidden="true">
                  <span style={{ width: `${Math.min(100, Math.max(0, x.fill))}%` }} />
                </span>
              )}
              {x.sub && <span className="tile-sub">{x.sub}</span>}
            </li>
          ))}
        </ul>
      )}

      <section className="start-card">
        <div className="start-row">
          <svg className="ecg-mini" viewBox="0 0 160 70" fill="none" aria-hidden="true">
            <path
              d="M0 40H44l6 0 6-18 8 40 6-52 8 36 4-6H92l6 0 4-10 6 22 4-10H160"
              stroke="currentColor"
              strokeWidth="3.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <div className="start-text">
            <h2>{t("ui.home.ready")}</h2>
            <p>
              {t("ui.home.areas_count", { count: areas.length })} ·{" "}
              <button type="button" className="linklike" aria-expanded={editing} onClick={() => setEditing((e) => !e)}>
                {editing ? t("ui.home.edit_done") : t("ui.home.edit")}
              </button>{" "}
              · {hasScore ? `${t("ui.home.score_line", { score })} · ` : ""}
              {last}
            </p>
          </div>
          <button type="button" className="primary big" disabled={areas.length === 0} onClick={start}>
            <Play size={18} weight="fill" aria-hidden />
            {t("ui.home.start")}
          </button>
        </div>

        {editing && (
          <div className="start-options">
            <fieldset className="areas">
              <legend>{t("ui.home.areas")}</legend>
              {AREAS.map((c) => (
                <label key={c} className={`chip${areas.includes(c) ? " on" : ""}`}>
                  <input type="checkbox" checked={areas.includes(c)} onChange={() => toggleArea(c)} />
                  {t(`category.${c}`)}
                </label>
              ))}
            </fieldset>
            {areas.length === 0 && <p className="muted">{t("ui.home.pick_one")}</p>}
            <label className="toggle">
              <input type="checkbox" checked={deep} onChange={(e) => setDeep(e.target.checked)} />
              <span className="switch" aria-hidden />
              {t("ui.home.deep")}
            </label>
          </div>
        )}
      </section>

      {onTroubleshoot && (
        <button type="button" className="linklike" onClick={onTroubleshoot}>
          <Wrench size={15} aria-hidden /> {t("ui.home.troubleshoot")}
        </button>
      )}
    </main>
  );
}

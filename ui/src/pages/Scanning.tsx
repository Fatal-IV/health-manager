import { useEffect, useRef, useState } from "react";
import { api, type Category, type Progress, type ScanMode, type ScanReport } from "../api";
import { t } from "../i18n";

/** What is being done decides the heading: one area, a few areas, a deep run or everything. */
export function scanTitleKey(mode: ScanMode, categories?: Category[]): string {
  if (categories && categories.length === 1) return `ui.scanning.title_area.${categories[0]}`;
  if (categories && categories.length > 1) return "ui.scanning.title_some";
  return mode === "deep" ? "ui.scanning.title_deep" : "ui.scanning.title";
}

export function Scanning({
  mode,
  categories,
  onDone,
  onCancel,
}: {
  mode: ScanMode;
  categories?: Category[];
  onDone: (r: ScanReport) => void;
  onCancel: () => void;
}) {
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const started = useRef(false);
  const alive = useRef(true);
  const cancelled = useRef(false);

  useEffect(() => {
    alive.current = true;
    let unlisten: (() => void) | undefined;
    api
      .onProgress((p) => alive.current && setProgress(p))
      .then((u) => {
        if (alive.current) unlisten = u;
        else u();
      })
      .catch(() => undefined);

    // React StrictMode runs effects twice in development; one scan only.
    if (!started.current) {
      started.current = true;
      (categories ? api.startScan(mode, categories) : api.startScan(mode)).then((r) => {
        if (!alive.current || cancelled.current) return;
        if (r.ok) onDone(r.value);
        else setError(r.error === "scan_in_progress" ? t("ui.scanning.busy") : t("ui.scanning.failed"));
      });
    }
    return () => {
      alive.current = false;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const cancel = () => {
    cancelled.current = true;
    void api.cancelScan();
    onCancel();
  };

  const title = t(scanTitleKey(mode, categories));
  const pct = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;
  return (
    <div className="stage home">
      <div className="stage-score" aria-hidden="true">
        <div className="bignum">{pct}</div>
        <p className="bignum-sub">%</p>
      </div>
      <section className="pane home-pane">
      <h1>{title}</h1>
      <progress value={pct} max={100} aria-label={title} />
      <p className="muted" aria-live="polite">
        {progress ? (
          <>
            {t(`category.${progress.category}`)}, {t("ui.scanning.progress", { done: progress.done, total: progress.total })}
          </>
        ) : (
          t("ui.scanning.starting")
        )}
      </p>
      {error && (
        <p className="err" role="alert">
          {error}
        </p>
      )}
      <button type="button" onClick={cancel}>
        {error ? t("ui.report.back") : t("ui.scanning.cancel")}
      </button>
      </section>
    </div>
  );
}

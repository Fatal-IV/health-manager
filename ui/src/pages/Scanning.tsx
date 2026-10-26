import { useEffect, useRef, useState } from "react";
import { api, type Progress, type ScanMode, type ScanReport } from "../api";
import { t } from "../i18n";

export function Scanning({
  mode,
  onDone,
  onCancel,
}: {
  mode: ScanMode;
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
      api.startScan(mode).then((r) => {
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

  const pct = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;
  return (
    <main className="scan">
      <h1>{t("ui.scanning.title")}</h1>
      <progress value={pct} max={100} aria-label={t("ui.scanning.title")} />
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
    </main>
  );
}

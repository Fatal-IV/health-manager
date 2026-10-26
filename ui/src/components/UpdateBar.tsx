import { DownloadSimple } from "@phosphor-icons/react";
import { useEffect, useState } from "react";
import { api, type UpdateInfo } from "../api";
import { t } from "../i18n";

/** Looks for a newer release once at start and offers it. A failed check is silent: it is not worth a warning. */
export function UpdateBar() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    void api.checkUpdate().then((r) => alive && r.ok && setUpdate(r.value));
    return () => {
      alive = false;
    };
  }, []);

  if (!update || dismissed) return null;

  const install = () => {
    setBusy(true);
    setFailed(false);
    // On success the app closes itself and the installer takes over.
    void api.installUpdate().then((r) => {
      if (!r.ok) {
        setBusy(false);
        setFailed(true);
      }
    });
  };

  return (
    <div className="update-bar" role="status">
      <DownloadSimple size={18} aria-hidden />
      <span>{failed ? t("ui.update.failed") : t("ui.update.available", { version: update.version })}</span>
      <button type="button" className="primary small" disabled={busy} onClick={install}>
        {busy ? t("ui.update.installing") : failed ? t("ui.update.retry") : t("ui.update.install")}
      </button>
      {!busy && (
        <button type="button" className="small" onClick={() => setDismissed(true)}>
          {t("ui.update.later")}
        </button>
      )}
    </div>
  );
}

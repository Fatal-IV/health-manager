import { ArrowRight } from "@phosphor-icons/react";
import type { TroubleshooterId } from "../api";
import { t } from "../i18n";

export const TROUBLESHOOTERS: TroubleshooterId[] = ["network", "windows_update", "system_files", "slowness"];

/** The list of guided troubleshooters; opening one starts its tests. */
export function TroubleshootPicker({ onPick, onBack }: { onPick: (id: TroubleshooterId) => void; onBack: () => void }) {
  return (
    <main>
      <h1>{t("ui.troubleshoot.picker_title")}</h1>
      <ul className="plain">
        {TROUBLESHOOTERS.map((id) => (
          <li key={id}>
            <button type="button" className="card" onClick={() => onPick(id)}>
              <strong>{t(`ui.troubleshoot.titles.${id}`)}</strong>
              <span className="muted">{t(`ui.troubleshoot.descriptions.${id}`)}</span>
              <ArrowRight size={16} aria-hidden />
            </button>
          </li>
        ))}
      </ul>
      <div className="row">
        <button type="button" onClick={onBack}>
          {t("ui.troubleshoot.back")}
        </button>
      </div>
    </main>
  );
}

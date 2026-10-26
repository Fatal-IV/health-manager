import { ArrowSquareOut, ShieldStar, Wrench } from "@phosphor-icons/react";
import { api, type Issue } from "../api";
import { StatusIcon } from "./StatusIcon";
import { t } from "../i18n";

export interface StartupEntry {
  id: string;
  name: string;
}

/** The startup issue carries its items as a JSON array in `params.items`. */
export function parseStartupItems(json: string | undefined): StartupEntry[] {
  if (!json) return [];
  try {
    const v: unknown = JSON.parse(json);
    if (!Array.isArray(v)) return [];
    return v
      .filter(
        (x): x is StartupEntry =>
          typeof x === "object" && x !== null && typeof (x as StartupEntry).id === "string" && typeof (x as StartupEntry).name === "string",
      )
      .slice(0, 100);
  } catch {
    return [];
  }
}

/** `arg` is the value for fixes that need one; `label` is what the preview should call the target. */
export type FixHandler = (issue: Issue, arg?: string, label?: string) => void;

/** Updates are installed by Windows itself; these issues point the user at its settings page. */
const OPENS_WINDOWS_UPDATE = new Set(["update.pending", "update.last_success", "update.never_updated"]);

/** A fix that needs no extra argument can be picked for a batch. */
export function canBatch(issue: Issue): boolean {
  return issue.fix !== null && issue.fix !== "disable_startup_item";
}

export function IssueItem({
  issue,
  onFix,
  selected,
  onSelect,
}: {
  issue: Issue;
  onFix: FixHandler;
  selected?: boolean;
  onSelect?: (checked: boolean) => void;
}) {
  const items = issue.fix === "disable_startup_item" ? parseStartupItems(issue.params.items) : [];
  return (
    <div className={`issue ${issue.status}`} data-testid={`issue-${issue.id}`}>
      <span className="issue-icon">
        <StatusIcon status={issue.status} />
      </span>
      <div>
      <div className="issue-title">
        <span className="issue-name">
          {onSelect && canBatch(issue) && (
            <input
              type="checkbox"
              className="pick"
              checked={!!selected}
              aria-label={`${t("ui.results.select_issue")}: ${t(issue.title_key, issue.params)}`}
              onChange={(e) => onSelect(e.target.checked)}
            />
          )}
          {t(issue.title_key, issue.params)}
        </span>
        <span className={`badge s-${issue.status}`}>{t(`status.${issue.status}`)}</span>
      </div>
      <p>{t(issue.why_key, issue.params)}</p>

      {items.length > 0 ? (
        <div>
          <div className="startup-label">{t("ui.results.startup_list")}</div>
          <ul className="plain">
            {items.map((it) => (
              <li key={it.id} className="row between">
                <span>{it.name}</span>
                <button type="button" className="small" aria-label={`${t("ui.results.fix_item")}: ${it.name}`} onClick={() => onFix(issue, it.id, it.name)}>
                  {t("ui.results.fix_item")}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : (
        issue.fix && (
          <button type="button" className="primary small" onClick={() => onFix(issue)}>
            {issue.fix === "read_boot_time" ? <ShieldStar size={15} aria-hidden /> : <Wrench size={15} aria-hidden />}
            {issue.fix === "read_boot_time" ? t("ui.results.read_boot_time") : t("ui.results.fix")}
          </button>
        )
      )}

      {OPENS_WINDOWS_UPDATE.has(issue.id) && (
        <button type="button" className="small" onClick={() => void api.openWindowsUpdate()}>
          <ArrowSquareOut size={15} aria-hidden />
          {t("ui.results.open_update")}
        </button>
      )}

      {issue.evidence && (
        <details>
          <summary>{t("ui.results.details")}</summary>
          <code>{issue.evidence}</code>
        </details>
      )}
      </div>
    </div>
  );
}

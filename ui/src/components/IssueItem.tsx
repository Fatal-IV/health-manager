import { Wrench } from "@phosphor-icons/react";
import type { Issue } from "../api";
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

export function IssueItem({ issue, onFix }: { issue: Issue; onFix: FixHandler }) {
  const items = issue.fix === "disable_startup_item" ? parseStartupItems(issue.params.items) : [];
  return (
    <div className={`issue ${issue.status}`} data-testid={`issue-${issue.id}`}>
      <span className="issue-icon">
        <StatusIcon status={issue.status} />
      </span>
      <div>
      <div className="issue-title">
        <span>{t(issue.title_key, issue.params)}</span>
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
            <Wrench size={15} aria-hidden />
            {t("ui.results.fix")}
          </button>
        )
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

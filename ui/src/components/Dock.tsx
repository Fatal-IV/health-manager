import type { Category, CheckResult } from "../api";
import { t } from "../i18n";
import { CATEGORY_ICONS, categoryStatus } from "./CategoryCard";

/** Floating glass dock: one tile per category, the selected one opens up with its name. */
export function Dock({
  items,
  selected,
  onSelect,
}: {
  items: { category: Category; results: CheckResult[] }[];
  selected: Category;
  onSelect: (c: Category) => void;
}) {
  return (
    <nav className="dock" aria-label={t("ui.results.areas")}>
      {items.map(({ category, results }) => {
        const status = categoryStatus(results);
        const count = results.reduce((n, r) => n + r.issues.length, 0);
        const Icon = CATEGORY_ICONS[category];
        const isSel = category === selected;
        const name = t(`category.${category}`);
        return (
          <button
            key={category}
            type="button"
            className={`dock-item s-${status}${isSel ? " sel" : ""}`}
            data-testid={`card-${category}`}
            data-status={status}
            aria-pressed={isSel}
            aria-label={name}
            title={name}
            onClick={() => onSelect(category)}
          >
            <span className="dock-ico">
              <Icon size={isSel ? 22 : 24} aria-hidden />
            </span>
            {isSel && <span className="dock-label">{name}</span>}
            {count > 0 && <span className="dock-badge">{count}</span>}
          </button>
        );
      })}
    </nav>
  );
}

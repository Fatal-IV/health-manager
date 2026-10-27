import type { Icon } from "@phosphor-icons/react";

export interface DockAction {
  key: string;
  label: string;
  icon: Icon;
  onClick: () => void;
  /** The main action: drawn in the accent colour, like the selected area in the results dock. */
  primary?: boolean;
  disabled?: boolean;
}

/** Floating glass dock of actions: icons only, the name opens to the left on hover or focus. */
export function ActionDock({ label, actions }: { label: string; actions: DockAction[] }) {
  return (
    <nav className="dock floating" aria-label={label}>
      {actions.map(({ key, label: name, icon: Icon, onClick, primary, disabled }) => (
        <button key={key} type="button" className={`dock-item${primary ? " sel" : ""}`} aria-label={name} disabled={disabled} onClick={onClick}>
          <span className="dock-pill">
            <span className="dock-ico">
              <Icon size={primary ? 28 : 22} aria-hidden />
            </span>
            <span className="dock-label" aria-hidden="true">
              {name}
            </span>
          </span>
        </button>
      ))}
    </nav>
  );
}

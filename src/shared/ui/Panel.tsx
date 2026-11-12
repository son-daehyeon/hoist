import type { ReactNode } from "react";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import "./Panel.css";

export function PanelHeader({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return (
    <div className="bs-panel-header">
      <span className="t-title-s">{title}</span>
      <span className="bs-panel-header-actions">{children}</span>
    </div>
  );
}

export function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: IconName;
  label: string;
  active?: boolean;
  onClick: () => void;
}) {
  return (
    <button type="button" className={`bs-nav-item t-label-m ${active ? "is-active" : ""}`} onClick={onClick}>
      <Icon name={icon} size={16} />
      {label}
    </button>
  );
}

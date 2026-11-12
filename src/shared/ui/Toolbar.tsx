import type { MouseEvent } from "react";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import "./Toolbar.css";

export function ToolbarDropdown({
  icon,
  label,
  onOpen,
  disabled,
  chevron = true,
}: {
  icon?: IconName;
  label: string;
  onOpen: (anchor: DOMRect) => void;
  disabled?: boolean;
  chevron?: boolean;
}) {
  return (
    <button
      type="button"
      className="bs-tb-dropdown t-label-s"
      disabled={disabled}
      onClick={(e: MouseEvent<HTMLButtonElement>) => onOpen(e.currentTarget.getBoundingClientRect())}
    >
      {icon && <Icon name={icon} size={14} />}
      {label}
      {chevron && <Icon name="chevron-down" size={12} />}
    </button>
  );
}

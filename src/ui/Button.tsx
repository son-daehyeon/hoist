import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import { Tooltip } from "./Tooltip";
import "./Button.css";

type NativeButton = Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children">;

export function Button({
  kind = "secondary",
  size = "m",
  icon,
  className = "",
  children,
  ...rest
}: NativeButton & {
  kind?: "primary" | "secondary" | "ghost" | "danger";
  size?: "m" | "l";
  icon?: IconName;
  children?: ReactNode;
}) {
  return (
    <button
      type="button"
      className={`bs-btn bs-btn-${kind} bs-btn-${size} ${className}`}
      {...rest}
    >
      {icon && <Icon name={icon} size={16} />}
      {children}
    </button>
  );
}

export function IconButton({
  icon,
  label,
  shortcut,
  active = false,
  size = 16,
  className = "",
  ...rest
}: NativeButton & {
  icon: IconName;
  label: string;
  shortcut?: string;
  active?: boolean;
  size?: number;
}) {
  return (
    <Tooltip label={label} shortcut={shortcut}>
      <button
        type="button"
        aria-label={label}
        aria-pressed={active}
        className={`bs-icon-btn ${active ? "is-active" : ""} ${className}`}
        {...rest}
      >
        <Icon name={icon} size={size} />
      </button>
    </Tooltip>
  );
}

export function ToolButton({
  icon,
  label,
  shortcut,
  active = false,
  ...rest
}: NativeButton & {
  icon: IconName;
  label: string;
  shortcut?: string;
  active?: boolean;
}) {
  return (
    <Tooltip label={label} shortcut={shortcut}>
      <button
        type="button"
        aria-pressed={active}
        className={`bs-tool-btn ${active ? "is-active" : ""}`}
        {...rest}
      >
        <Icon name={icon} size={20} />
        <span className="t-micro">{label}</span>
      </button>
    </Tooltip>
  );
}

import { useEffect, useState, type MouseEvent } from "react";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import "./Toolbar.css";

export function ToolbarField({
  prefix,
  value,
  onCommit,
  width = 68,
  disabled,
}: {
  prefix: string;
  value: number;
  onCommit: (v: number) => void;
  width?: number;
  disabled?: boolean;
}) {
  const shown = String(Math.round(value));
  const [draft, setDraft] = useState(shown);
  useEffect(() => setDraft(shown), [shown]);

  const commit = () => {
    const n = Number(draft);
    if (Number.isFinite(n) && draft.trim() !== "" && String(n) !== shown) onCommit(n);
    else setDraft(shown);
  };

  return (
    <label className="bs-tb-field" style={{ width }}>
      <span className="t-micro">{prefix}</span>
      <input
        className="t-label-s"
        value={draft}
        disabled={disabled}
        inputMode="decimal"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            setDraft(shown);
            e.currentTarget.blur();
          }
        }}
      />
    </label>
  );
}

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

export function ToolbarSwatch({
  color,
  onChange,
  disabled,
}: {
  color: string;
  onChange: (hex: string) => void;
  disabled?: boolean;
}) {
  return (
    <label className="bs-tb-swatch t-label-s">
      <span className="bs-tb-swatch-chip" style={{ background: color }} />
      {color.replace("#", "").toUpperCase()}
      <input
        type="color"
        value={color}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value.toUpperCase())}
      />
    </label>
  );
}

export function ToolbarDivider() {
  return <span className="bs-tb-divider" />;
}

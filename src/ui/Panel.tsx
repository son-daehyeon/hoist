import { useState, type DragEvent, type MouseEvent, type ReactNode } from "react";
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

export function SectionHeader({
  label,
  open,
  onToggle,
}: {
  label: string;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <button type="button" className="bs-section-header t-label-s" onClick={onToggle}>
      <Icon name={open ? "chevron-down" : "chevron-right"} size={12} />
      {label}
    </button>
  );
}

export function LayerRow({
  depth,
  name,
  icon,
  tag,
  pinned,
  visible,
  selected,
  collapsed,
  onToggleCollapsed,
  onSelect,
  onToggleVisible,
  onRename,
  drop,
  dragHandlers,
}: {
  depth: number;
  name: string;
  icon: IconName;
  tag?: string;
  pinned?: boolean;
  visible: boolean;
  selected: boolean;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
  onSelect: (e: MouseEvent) => void;
  onToggleVisible: () => void;
  onRename: (name: string) => void;
  drop?: "above" | "below" | null;
  dragHandlers?: {
    draggable: boolean;
    onDragStart: (e: DragEvent) => void;
    onDragOver: (e: DragEvent) => void;
    onDragLeave: () => void;
    onDrop: (e: DragEvent) => void;
  };
}) {
  const [editing, setEditing] = useState(false);
  const isGroup = onToggleCollapsed !== undefined;

  return (
    <div
      className={`bs-layer-row ${selected ? "is-selected" : ""} ${visible ? "" : "is-hidden"} ${drop ? `drop-${drop}` : ""}`}
      style={{ paddingLeft: depth === 0 ? 6 : 6 + depth * 18 }}
      onClick={onSelect}
      onDoubleClick={() => setEditing(true)}
      {...dragHandlers}
    >
      <span
        className="bs-layer-chevron"
        onClick={(e) => {
          if (!isGroup) return;
          e.stopPropagation();
          onToggleCollapsed();
        }}
        onDoubleClick={(e) => e.stopPropagation()}
      >
        {isGroup && <Icon name={collapsed ? "chevron-right" : "chevron-down"} size={12} />}
      </span>
      <Icon name={icon} size={14} className="bs-layer-icon" />
      {editing ? (
        <input
          className="bs-layer-input t-body-m"
          autoFocus
          defaultValue={name}
          onClick={(e) => e.stopPropagation()}
          onFocus={(e) => e.currentTarget.select()}
          onBlur={(e) => {
            setEditing(false);
            const v = e.currentTarget.value.trim();
            if (v && v !== name) onRename(v);
          }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") {
              e.currentTarget.value = name;
              e.currentTarget.blur();
            }
          }}
        />
      ) : (
        <span className="bs-layer-name t-body-m">{name}</span>
      )}
      {tag && <span className="t-micro c-tertiary bs-layer-tag">{tag}</span>}
      {pinned && <Icon name="lock" size={12} className="bs-layer-pin" />}
      <button
        type="button"
        className="bs-layer-eye"
        aria-label={visible ? "숨기기" : "보이기"}
        onClick={(e) => {
          e.stopPropagation();
          onToggleVisible();
        }}
      >
        <Icon name={visible ? "eye" : "eye-off"} size={14} />
      </button>
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

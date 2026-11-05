import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import "./Menu.css";

export type MenuItem =
  | "divider"
  | { heading: string }
  | {
      label: string;
      icon?: IconName;
      shortcut?: string;
      danger?: boolean;
      checked?: boolean;
      disabled?: boolean;
      onSelect: () => void;
    };

export function Menu({
  anchor,
  items,
  onClose,
  align = "start",
  width = 200,
  point = false,
}: {
  anchor: DOMRect;
  items: MenuItem[];
  onClose: () => void;
  align?: "start" | "end";
  width?: number;
  point?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: 0, top: 0 });

  useLayoutEffect(() => {
    const h = ref.current?.offsetHeight ?? 0;
    if (point) {
      const left = anchor.left + width > window.innerWidth - 4 ? anchor.left - width : anchor.left;
      const top = anchor.top + h > window.innerHeight - 4 ? anchor.top - h : anchor.top;
      setPos({ left: Math.max(4, left), top: Math.max(4, top) });
      return;
    }
    let left = align === "start" ? anchor.left : anchor.right - width;
    left = Math.max(4, Math.min(left, window.innerWidth - width - 4));
    let top = anchor.bottom + 4;
    if (top + h > window.innerHeight - 4) top = Math.max(4, anchor.top - h - 4);
    setPos({ left, top });
  }, [anchor, align, width, point]);

  useEffect(() => {
    const down = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("keydown", key, true);
    };
  }, [onClose]);

  return createPortal(
    <div
      ref={ref}
      className="bs-menu"
      role="menu"
      style={{ ...pos, width }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((it, i) =>
        it === "divider" ? (
          <div key={i} className="bs-menu-divider" />
        ) : "heading" in it ? (
          <div key={i} className="bs-menu-heading t-label-m" role="presentation">
            {it.heading}
          </div>
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            disabled={it.disabled}
            className={`bs-menu-item t-body-m ${it.danger ? "is-danger" : ""}`}
            onClick={() => {
              onClose();
              it.onSelect();
            }}
          >
            <span className="bs-menu-icon">
              {it.checked ? <Icon name="check" size={16} /> : it.icon && <Icon name={it.icon} size={16} />}
            </span>
            <span className="bs-menu-label">{it.label}</span>
            {it.shortcut && <span className="t-micro c-tertiary">{it.shortcut}</span>}
          </button>
        ),
      )}
    </div>,
    document.body,
  );
}

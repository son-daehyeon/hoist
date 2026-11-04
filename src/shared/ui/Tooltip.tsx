import { useRef, useState, type CSSProperties, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./Tooltip.css";

export function TooltipBubble({
  label,
  shortcut,
  style,
}: {
  label: string;
  shortcut?: string;
  style?: CSSProperties;
}) {
  return (
    <div className="bs-tooltip" style={style} role="tooltip">
      {label}
      {shortcut && <span className="bs-tooltip-key">{shortcut}</span>}
    </div>
  );
}

export function Tooltip({
  label,
  shortcut,
  children,
}: {
  label: string;
  shortcut?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const timer = useRef<number>(0);
  const [pos, setPos] = useState<{ x: number; y: number; above: boolean } | null>(null);

  const show = () => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      const r = ref.current?.getBoundingClientRect();
      if (!r) return;
      const above = r.bottom + 36 > window.innerHeight;
      setPos({ x: r.left + r.width / 2, y: above ? r.top - 6 : r.bottom + 6, above });
    }, 400);
  };
  const hide = () => {
    window.clearTimeout(timer.current);
    setPos(null);
  };

  return (
    <span
      ref={ref}
      className="bs-tooltip-anchor"
      onMouseEnter={show}
      onMouseLeave={hide}
      onPointerDown={hide}
    >
      {children}
      {pos &&
        createPortal(
          <TooltipBubble
            label={label}
            shortcut={shortcut}
            style={{
              position: "fixed",
              left: Math.max(4, Math.min(pos.x, window.innerWidth - 4)),
              top: pos.y,
              transform: `translate(-50%, ${pos.above ? "-100%" : "0"})`,
            }}
          />,
          document.body,
        )}
    </span>
  );
}

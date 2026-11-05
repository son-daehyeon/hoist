import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./Popover.css";

export function Popover({
  anchor,
  onClose,
  width = 240,
  side = "bottom",
  keepOpen,
  label,
  children,
}: {
  anchor: HTMLElement;
  onClose: () => void;
  width?: number;
  side?: "bottom" | "right";
  keepOpen?: () => boolean;
  label: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    const place = () => {
      const a = anchor.getBoundingClientRect();
      const h = ref.current?.offsetHeight ?? 0;
      const maxTop = Math.max(8, window.innerHeight - h - 8);
      if (side === "right") {
        const edge = anchor.closest("[data-popover-edge]")?.getBoundingClientRect().right ?? a.right;
        setPos({
          left: Math.min(edge + 8, window.innerWidth - width - 8),
          top: Math.min(Math.max(8, a.top - 8), maxTop),
        });
        return;
      }
      const below = a.bottom + 4;
      const top = below + h > window.innerHeight - 8 ? a.top - h - 4 : below;
      setPos({
        left: Math.max(8, Math.min(a.left, window.innerWidth - width - 8)),
        top: Math.min(Math.max(8, top), maxTop),
      });
    };
    place();
    const ro = new ResizeObserver(place);
    if (ref.current) ro.observe(ref.current);
    window.addEventListener("resize", place);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", place);
    };
  }, [anchor, side, width]);

  useEffect(() => {
    const down = (e: PointerEvent) => {
      const t = e.target as Node;
      if (keepOpen?.() || ref.current?.contains(t) || anchor.contains(t)) return;
      onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || keepOpen?.()) return;
      e.stopPropagation();
      onClose();
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("keydown", key, true);
    };
  }, [anchor, onClose, keepOpen]);

  return createPortal(
    <div
      ref={ref}
      className="bs-popover"
      role="dialog"
      aria-label={label}
      style={{ width, left: pos?.left ?? -9999, top: pos?.top ?? -9999 }}
    >
      {children}
    </div>,
    document.body,
  );
}

import { useState, type PointerEvent } from "react";

type Options = { min: number; max: number; initial: number; edge: "left" | "right" };

export function useColumnWidth(key: string, { min, max, initial, edge }: Options) {
  const clamp = (v: number) => Math.min(max, Math.max(min, v));
  const [width, setWidth] = useState(() => clamp(Number(localStorage.getItem(key)) || initial));
  const [start, setStart] = useState<{ x: number; width: number } | null>(null);

  const handle = {
    onPointerDown: (e: PointerEvent<HTMLDivElement>) => {
      e.currentTarget.setPointerCapture(e.pointerId);
      setStart({ x: e.clientX, width });
    },
    onPointerMove: (e: PointerEvent<HTMLDivElement>) => {
      if (start) setWidth(clamp(start.width + (edge === "left" ? start.x - e.clientX : e.clientX - start.x)));
    },
    onPointerUp: () => {
      if (!start) return;
      setStart(null);
      localStorage.setItem(key, String(width));
    },
  };

  return { width, resizing: start !== null, edge, handle };
}

import { useState, type PointerEvent } from "react";

const KEY = "hoist.assistant-width";
const MIN = 360;
const MAX = 720;
const DEFAULT = 440;
const clamp = (v: number) => Math.min(MAX, Math.max(MIN, v));

export function usePanelWidth() {
  const [width, setWidth] = useState(() => clamp(Number(localStorage.getItem(KEY)) || DEFAULT));
  const [start, setStart] = useState<{ x: number; width: number } | null>(null);

  const handle = {
    onPointerDown: (e: PointerEvent<HTMLDivElement>) => {
      e.currentTarget.setPointerCapture(e.pointerId);
      setStart({ x: e.clientX, width });
    },
    onPointerMove: (e: PointerEvent<HTMLDivElement>) => {
      if (start) setWidth(clamp(start.width + start.x - e.clientX));
    },
    onPointerUp: () => {
      if (!start) return;
      setStart(null);
      localStorage.setItem(KEY, String(width));
    },
  };

  return { width, resizing: start !== null, handle };
}

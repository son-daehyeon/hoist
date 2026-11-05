import { useState, type PointerEvent } from "react";

const KEY = "hoist.props-height";
const MIN = 120;
const LAYERS_MIN = 140;
const DEFAULT = 380;

export function usePropsHeight() {
  const [height, setHeight] = useState(() => Math.max(MIN, Number(localStorage.getItem(KEY)) || DEFAULT));
  const [start, setStart] = useState<{ y: number; height: number; max: number } | null>(null);

  const handle = {
    onPointerDown: (e: PointerEvent<HTMLDivElement>) => {
      e.currentTarget.setPointerCapture(e.pointerId);
      const column = e.currentTarget.parentElement?.clientHeight ?? window.innerHeight;
      setStart({ y: e.clientY, height, max: Math.max(MIN, column - LAYERS_MIN) });
    },
    onPointerMove: (e: PointerEvent<HTMLDivElement>) => {
      if (start) setHeight(Math.min(start.max, Math.max(MIN, start.height + e.clientY - start.y)));
    },
    onPointerUp: () => {
      if (!start) return;
      setStart(null);
      localStorage.setItem(KEY, String(height));
    },
  };

  return { height, resizing: start !== null, handle };
}

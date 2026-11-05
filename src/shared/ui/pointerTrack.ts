import type { PointerEvent } from "react";

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

export function track<T extends HTMLElement>(
  update: (fx: number, fy: number, e: PointerEvent<T>) => void,
  done: () => void,
  rectOf: (el: T) => DOMRect = (el) => el.getBoundingClientRect(),
) {
  const at = (e: PointerEvent<T>) => {
    const r = rectOf(e.currentTarget);
    update(clamp01((e.clientX - r.left) / r.width), clamp01((e.clientY - r.top) / r.height), e);
  };
  return {
    onPointerDown: (e: PointerEvent<T>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      e.currentTarget.setPointerCapture(e.pointerId);
      at(e);
    },
    onPointerMove: (e: PointerEvent<T>) => {
      if (e.currentTarget.hasPointerCapture(e.pointerId)) at(e);
    },
    onPointerUp: (e: PointerEvent<T>) => {
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      e.currentTarget.releasePointerCapture(e.pointerId);
      done();
    },
  };
}

import type { Box } from "@/entities/layer";

export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

export type RasterShift = { dx: number; dy: number; sx: number; sy: number };

export function rasterShift(raster: Box, from: Box, to: Box, zoom: number): RasterShift {
  const sx = to.w / (from.w || 1);
  const sy = to.h / (from.h || 1);
  return {
    dx: (to.x + (raster.x - from.x) * sx - raster.x) * zoom,
    dy: (to.y + (raster.y - from.y) * sy - raster.y) * zoom,
    sx,
    sy,
  };
}

export const shiftCss = ({ dx, dy, sx, sy }: RasterShift) =>
  `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`;

const ARROWS: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
};

export function nudged(box: Box, key: string, big: boolean): Box | null {
  const d = ARROWS[key];
  if (!d) return null;
  const step = big ? 10 : 1;
  return { ...box, x: box.x + d[0] * step, y: box.y + d[1] * step };
}

export function resize(b: Box, h: Handle, dx: number, dy: number): Box {
  const west = h.includes("w");
  const north = h.includes("n");
  let w = west ? b.w - dx : h.includes("e") ? b.w + dx : b.w;
  let hh = north ? b.h - dy : h.includes("s") ? b.h + dy : b.h;
  w = Math.max(1, w);
  hh = Math.max(1, hh);
  return { x: west ? b.x + b.w - w : b.x, y: north ? b.y + b.h - hh : b.y, w, h: hh };
}

export function withRatio(b0: Box, b: Box, h: Handle): Box {
  const r = b0.w / (b0.h || 1);
  const horiz = h.includes("e") || h.includes("w");
  const vert = h.includes("n") || h.includes("s");
  let w = b.w;
  let hh = b.h;
  if (horiz && (!vert || w / b0.w >= hh / b0.h)) hh = w / r;
  else w = hh * r;
  w = Math.max(1, w);
  hh = Math.max(1, hh);
  const x = h.includes("w") ? b0.x + b0.w - w : h.includes("e") ? b0.x : b0.x + (b0.w - w) / 2;
  const y = h.includes("n") ? b0.y + b0.h - hh : h.includes("s") ? b0.y : b0.y + (b0.h - hh) / 2;
  return { x, y, w, h: hh };
}

export const lockAxis = (dx: number, dy: number): [number, number] =>
  Math.abs(dx) >= Math.abs(dy) ? [dx, 0] : [0, dy];

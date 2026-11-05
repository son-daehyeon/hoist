export type Box = { x: number; y: number; w: number; h: number };
export type AlignMode = "left" | "center" | "right" | "top" | "middle" | "bottom";
export type Move = { id: string; dx: number; dy: number };
type Item = { id: string; box: Box };

export function aabb(b: Box, deg: number): Box {
  const r = (deg * Math.PI) / 180;
  const [c, s] = [Math.abs(Math.cos(r)), Math.abs(Math.sin(r))];
  const hw = (b.w * c + b.h * s) / 2;
  const hh = (b.w * s + b.h * c) / 2;
  const cx = b.x + b.w / 2;
  const cy = b.y + b.h / 2;
  return { x: cx - hw, y: cy - hh, w: hw * 2, h: hh * 2 };
}

export function union(boxes: Box[]): Box {
  const x = Math.min(...boxes.map((b) => b.x));
  const y = Math.min(...boxes.map((b) => b.y));
  const r = Math.max(...boxes.map((b) => b.x + b.w));
  const bottom = Math.max(...boxes.map((b) => b.y + b.h));
  return { x, y, w: r - x, h: bottom - y };
}

export function alignMoves(items: Item[], ref: Box, mode: AlignMode): Move[] {
  return items.map(({ id, box: b }) => {
    const dx =
      mode === "left"
        ? ref.x - b.x
        : mode === "center"
          ? ref.x + ref.w / 2 - (b.x + b.w / 2)
          : mode === "right"
            ? ref.x + ref.w - (b.x + b.w)
            : 0;
    const dy =
      mode === "top"
        ? ref.y - b.y
        : mode === "middle"
          ? ref.y + ref.h / 2 - (b.y + b.h / 2)
          : mode === "bottom"
            ? ref.y + ref.h - (b.y + b.h)
            : 0;
    return { id, dx, dy };
  });
}

export function distributeMoves(items: Item[], axis: "x" | "y"): Move[] {
  const pos = (b: Box) => (axis === "x" ? b.x : b.y);
  const size = (b: Box) => (axis === "x" ? b.w : b.h);
  const sorted = [...items].sort((a, b) => pos(a.box) - pos(b.box));
  const first = sorted[0].box;
  const last = sorted[sorted.length - 1].box;
  const span = pos(last) + size(last) - pos(first);
  const gap = (span - sorted.reduce((sum, i) => sum + size(i.box), 0)) / (sorted.length - 1);
  let at = pos(first);
  return sorted.map(({ id, box }) => {
    const d = at - pos(box);
    at += size(box) + gap;
    return { id, dx: axis === "x" ? d : 0, dy: axis === "y" ? d : 0 };
  });
}

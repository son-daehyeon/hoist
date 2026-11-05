import type { Editing } from "@/entities/editor";
import type { ImageProps, Layer } from "@/shared/api";
import type { Handle, Params } from "./drag";

export type Edges = [number, number, number, number];
export type CropMap = { ox: number; oy: number; sx: number; sy: number };
type Image = Pick<ImageProps, "natural_w_px" | "natural_h_px" | "fit" | "crop_px">;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function edgesOf(k: Image): Edges {
  const c = k.crop_px;
  return c && c[2] > 0 && c[3] > 0
    ? [c[0], c[1], c[0] + c[2], c[1] + c[3]]
    : [0, 0, k.natural_w_px, k.natural_h_px];
}

export function cropMap(k: Image, w: number, h: number): CropMap {
  const [l, t, r, b] = edgesOf(k);
  let sx = w / (r - l);
  let sy = h / (b - t);
  if (k.fit !== "fill") sx = sy = k.fit === "contain" ? Math.min(sx, sy) : Math.max(sx, sy);
  return { sx, sy, ox: (w - (r - l) * sx) / 2 - l * sx, oy: (h - (b - t) * sy) / 2 - t * sy };
}

export const cropBox = ([l, t, r, b]: Edges, m: CropMap) => ({
  x: m.ox + l * m.sx,
  y: m.oy + t * m.sy,
  w: (r - l) * m.sx,
  h: (b - t) * m.sy,
});

export function dragEdges(
  e: Edges,
  handle: Handle | "move",
  du: number,
  dv: number,
  nw: number,
  nh: number,
): Edges {
  let [l, t, r, b] = e;
  if (handle === "move") {
    const dx = clamp(du, -l, nw - r);
    const dy = clamp(dv, -t, nh - b);
    return [l + dx, t + dy, r + dx, b + dy];
  }
  if (handle.includes("w")) l = clamp(l + du, 0, r - 1);
  if (handle.includes("e")) r = clamp(r + du, l + 1, nw);
  if (handle.includes("n")) t = clamp(t + dv, 0, b - 1);
  if (handle.includes("s")) b = clamp(b + dv, t + 1, nh);
  return [l, t, r, b];
}

export function cropPx(e: Edges, nw: number, nh: number): [number, number, number, number] | null {
  const [l, t, r, b] = e.map(Math.round);
  return l <= 0 && t <= 0 && r >= nw && b >= nh ? null : [l, t, r - l, b - t];
}

export function cropFrame(f: Params, box: { x: number; y: number; w: number; h: number }): Params {
  const rad = (f.deg * Math.PI) / 180;
  const [dx, dy] = [box.x + box.w / 2 - f.w / 2, box.y + box.h / 2 - f.h / 2];
  const cx = f.x + f.w / 2 + dx * Math.cos(rad) - dy * Math.sin(rad);
  const cy = f.y + f.h / 2 + dx * Math.sin(rad) + dy * Math.cos(rad);
  return { x: cx - box.w / 2, y: cy - box.h / 2, w: box.w, h: box.h, deg: f.deg };
}

const XLINK = "http://www.w3.org/1999/xlink";

export function cropStart(l: Layer): Editing | null {
  if (l.kind.type !== "image") return null;
  const img = document.querySelector(`.cv-svg [data-layer="${CSS.escape(l.id)}"] image`);
  const href = img?.getAttribute("href") ?? img?.getAttributeNS(XLINK, "href") ?? null;
  return { kind: "crop", id: l.id, edges: edgesOf(l.kind), href };
}

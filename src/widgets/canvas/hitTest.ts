import type { Pt } from "@/entities/path";
import type { Layer, LayerId } from "@/shared/api";
import { isEffectivelyHidden, paintOrder } from "@/shared/lib";
import { aabb, centerOf, rotatePoint } from "./geometry";
import { frameOf } from "./selection";

const PROBES: [number, number][] = [
  [0, 0],
  [3, 0],
  [-3, 0],
  [0, 3],
  [0, -3],
  [2, 2],
  [-2, -2],
  [2, -2],
  [-2, 2],
];

const pickable = (layers: Layer[], l: Layer | undefined): l is Layer =>
  !!l && !l.locked && !isEffectivelyHidden(layers, l);

export function pickAt(
  svg: SVGSVGElement | null,
  layers: Layer[],
  clientX: number,
  clientY: number,
  wide: boolean,
) {
  if (!svg) return null;
  for (const [ox, oy] of wide ? PROBES : PROBES.slice(0, 1)) {
    for (const el of document.elementsFromPoint(clientX + ox, clientY + oy)) {
      if (!svg.contains(el)) continue;
      const node = el.closest("[data-layer]");
      const id = node?.getAttribute("data-layer") ?? "";
      const layer = layers.find((x) => x.id === id);
      if (node && pickable(layers, layer)) return { layer, node };
    }
  }
  return null;
}

export function boxHit(layers: Layer[], mm: Pt) {
  const order = paintOrder(layers);
  for (let i = order.length - 1; i >= 0; i--) {
    const l = order[i];
    if (l.kind.type === "group" || !pickable(layers, l)) continue;
    const b = aabb(frameOf(layers, l));
    if (mm.x >= b.x && mm.x <= b.x + b.w && mm.y >= b.y && mm.y <= b.y + b.h) return l;
  }
  return null;
}

export function selectedOwner(node: Element | null, selection: LayerId[]) {
  for (let n = node; n; n = n.parentElement?.closest("[data-layer]") ?? null) {
    const id = n.getAttribute("data-layer");
    if (id && selection.includes(id)) return id;
  }
  return null;
}

export function inFrame(layers: Layer[], l: Layer, mm: Pt) {
  const f = frameOf(layers, l);
  const q = rotatePoint(mm, centerOf(f), -f.deg);
  return q.x >= f.x && q.x <= f.x + f.w && q.y >= f.y && q.y <= f.y + f.h;
}

export const insideSelection = (layers: Layer[], selection: LayerId[], mm: Pt) =>
  selection.some((id) => {
    const l = layers.find((x) => x.id === id);
    return !!l && inFrame(layers, l, mm);
  });

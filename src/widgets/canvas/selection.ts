import type { Layer, LayerId } from "@/shared/api";
import { aabb, IDENTITY, localMatrix, mul, placed, unionBox, type Matrix, type Params } from "./drag";

export type Edits = { base: Record<LayerId, Params>; next: Record<LayerId, Params>; copy?: boolean };

export const localOf = (l: Layer): Params => ({
  x: l.x_mm,
  y: l.y_mm,
  w: l.w_mm,
  h: l.h_mm,
  deg: l.rotation_deg,
});

export function parentMatrix(layers: Layer[], l: Layer): Matrix {
  const chain: Layer[] = [];
  for (let p = layers.find((x) => x.id === l.parent); p; p = layers.find((x) => x.id === p?.parent)) {
    chain.unshift(p);
  }
  return chain.reduce((m, a) => mul(m, localMatrix(localOf(a))), IDENTITY);
}

export const paramsOf = (layers: Layer[], l: Layer): Params => placed(parentMatrix(layers, l), localOf(l));

export function hasAncestor(layers: Layer[], l: Layer, ids: ReadonlySet<LayerId>) {
  for (let p = l.parent; p; p = layers.find((x) => x.id === p)?.parent ?? null) if (ids.has(p)) return true;
  return false;
}

export function movable(layers: Layer[], ids: LayerId[]): Layer[] {
  const set = new Set(ids);
  return layers.filter((l) => set.has(l.id) && !l.locked && !hasAncestor(layers, l, set));
}

export function canTransform(layers: Layer[], ids: LayerId[]): boolean {
  const picked = layers.filter((l) => ids.includes(l.id));
  return picked.length > 0 && picked.every((l) => !l.locked && l.kind.type !== "group");
}

export function frameFor(params: Params[]): Params | null {
  if (params.length === 0) return null;
  if (params.length === 1) return params[0];
  return { ...unionBox(params.map(aabb)), deg: 0 };
}

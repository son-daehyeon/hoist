import type { Document, Layer, Op, Paint } from "@/shared/api";
import { normalizeHex } from "@/shared/lib";

export type Patch = Extract<Op, { op: "update_layer" }>["patch"];
export type Ops = (list: Op[]) => Promise<boolean>;

export const updateAll = (ops: Ops, layers: Layer[], fn: (l: Layer) => Patch) =>
  void ops(layers.map((l) => ({ op: "update_layer" as const, id: l.id, patch: fn(l) })));

export function common<T>(items: T[], get: (x: T) => number): number | null {
  if (items.length === 0) return null;
  const first = get(items[0]);
  return items.every((x) => Math.abs(get(x) - first) < 0.05) ? first : null;
}

export const solid = (color: string, opacity = 1): Paint => ({ type: "solid", color, opacity });

export const paintLabel = (p: Paint) =>
  p.type === "solid" ? p.color.replace("#", "").toUpperCase() : "그라디언트";

export const moveItem = <T>(list: T[], from: number, to: number) => {
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
};

export function docColors(doc: Document): string[] {
  const out = new Set<string>();
  const add = (c: string) => {
    const n = normalizeHex(c);
    if (n) out.add(n);
  };
  const paint = (p: Paint | null | undefined) => {
    if (p?.type === "solid") add(p.color);
    else p?.stops.forEach((s) => add(s.color));
  };
  paint(doc.artboard.background);
  for (const l of doc.layers) {
    l.fills.forEach(paint);
    l.strokes.forEach((s) => paint(s.paint));
    l.effects.forEach((e) => e.type === "drop_shadow" && add(e.color));
  }
  return [...out].slice(0, 24);
}

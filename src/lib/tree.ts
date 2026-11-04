import type { Layer, LayerId } from "../types";

export function childrenOf(layers: Layer[], parent: LayerId | null) {
  return layers.filter((l) => (l.parent ?? null) === parent);
}

export function paintOrder(layers: Layer[]): Layer[] {
  const walk = (parent: LayerId | null): Layer[] =>
    childrenOf(layers, parent).flatMap((l) => [l, ...walk(l.id)]);
  return walk(null);
}

export function isEffectivelyHidden(layers: Layer[], layer: Layer): boolean {
  let cur: Layer | undefined = layer;
  while (cur) {
    if (!cur.visible) return true;
    const p: LayerId | null | undefined = cur.parent;
    cur = p ? layers.find((l) => l.id === p) : undefined;
  }
  return false;
}

export function nextLayerId(layers: Layer[]) {
  const max = layers.reduce((m, l) => {
    const n = /^L(\d+)$/.exec(l.id);
    return n ? Math.max(m, Number(n[1])) : m;
  }, 0);
  return `L${max + 1}`;
}

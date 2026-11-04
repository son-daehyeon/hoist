import type { Layer, LayerId } from "../api";

export function childrenOf(layers: Layer[], parent: LayerId | null) {
  return layers.filter((l) => (l.parent ?? null) === parent);
}

export function paintOrder(layers: Layer[]): Layer[] {
  const walk = (parent: LayerId | null): Layer[] =>
    childrenOf(layers, parent).flatMap((l) => [l, ...walk(l.id)]);
  return walk(null);
}

function ancestors(layers: Layer[], layer: Layer): Layer[] {
  const out: Layer[] = [];
  let p = layers.find((l) => l.id === layer.parent);
  while (p) {
    out.push(p);
    const up: LayerId | null | undefined = p.parent;
    p = up ? layers.find((l) => l.id === up) : undefined;
  }
  return out;
}

export function isEffectivelyHidden(layers: Layer[], layer: Layer): boolean {
  return !layer.visible || ancestors(layers, layer).some((a) => !a.visible);
}

export function parentOffset(layers: Layer[], layer: Layer) {
  return ancestors(layers, layer).reduce((o, a) => ({ x: o.x + a.x_mm, y: o.y + a.y_mm }), { x: 0, y: 0 });
}

export function nextLayerId(layers: Layer[]) {
  const max = layers.reduce((m, l) => {
    const n = /^L(\d+)$/.exec(l.id);
    return n ? Math.max(m, Number(n[1])) : m;
  }, 0);
  return `L${max + 1}`;
}

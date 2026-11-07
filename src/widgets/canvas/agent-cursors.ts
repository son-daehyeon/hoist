import type { Layer } from "@/shared/api";
import { centerOf, pointOn, type Params } from "./drag.ts";

export type Work = "text" | "image" | "shape" | "group";
export const WORK_LABEL: Record<Work, string> = { text: "글자", image: "그림", shape: "도형", group: "묶음" };

export const SLOTS = 4;
const FLY = 650;
const STAGGER = 350;
const HOLD = 600;
const ACT = { add: 1800, move: 1600, touch: 900 } as const;

export type Pt = { x: number; y: number };
export type Change = { kind: keyof typeof ACT; id: string; work: Work; from: Params; to: Params };
export type Clip = Change & { slot: number; start: number; end: number };
export type Frame = {
  cursor: Pt;
  box: Params | null;
  badge: string | null;
  layer: Params | null;
  hide: boolean;
  press: boolean;
  done: boolean;
};
export type Placer = (layers: Layer[], l: Layer) => Params;

const KEYS = ["x", "y", "w", "h", "deg"] as const;
const workOf = (l: Layer): Work => (l.kind.type === "path" ? "shape" : l.kind.type);
const near = (a: Params, b: Params) => KEYS.every((k) => Math.abs(a[k] - b[k]) < 0.01);

export function diffLayers(prev: Layer[], next: Layer[], at: Placer): Change[] {
  const before = new Map(prev.map((l) => [l.id, l]));
  const after = new Map(next.map((l) => [l.id, l]));
  const out: Change[] = [];
  for (const l of next) {
    const to = at(next, l);
    const old = before.get(l.id);
    if (!old) out.push({ kind: "add", id: l.id, work: workOf(l), from: to, to });
    else {
      const from = at(prev, old);
      if (!near(from, to)) out.push({ kind: "move", id: l.id, work: workOf(l), from, to });
      else if (JSON.stringify(old) !== JSON.stringify(l))
        out.push({ kind: "touch", id: l.id, work: workOf(l), from: to, to });
    }
  }
  for (const l of prev) {
    if (after.has(l.id)) continue;
    const from = at(prev, l);
    out.push({ kind: "touch", id: l.id, work: workOf(l), from, to: from });
  }
  const changed = new Set(out.map((c) => c.id));
  return out.filter((c) => !changed.has((after.get(c.id) ?? before.get(c.id))?.parent ?? ""));
}

export function planBatch(changes: Change[], now: number, free: number[]): { clips: Clip[]; free: number[] } {
  const next = [...free];
  const clips = changes.map((c, i) => {
    const slot = next.indexOf(Math.min(...next));
    const start = Math.max(now + i * STAGGER, next[slot]);
    const end = start + FLY + ACT[c.kind] + HOLD;
    next[slot] = end;
    return { ...c, slot, start, end };
  });
  return { clips, free: next };
}

const clamp01 = (u: number) => Math.min(1, Math.max(0, u));
const easeOut = (u: number) => 1 - (1 - u) ** 3;
const easeInOut = (u: number) => (u < 0.5 ? 4 * u * u * u : 1 - (-2 * u + 2) ** 3 / 2);
const lerp = (a: number, b: number, u: number) => a + (b - a) * u;
const lerpParams = (a: Params, b: Params, u: number): Params => ({
  x: lerp(a.x, b.x, u),
  y: lerp(a.y, b.y, u),
  w: lerp(a.w, b.w, u),
  h: lerp(a.h, b.h, u),
  deg: lerp(a.deg, b.deg, u),
});
const resizes = (c: Change) => c.kind === "add" || c.from.w !== c.to.w || c.from.h !== c.to.h;
const grip = (c: Change, p: Params): Pt => (resizes(c) ? pointOn(p, 1, 1) : centerOf(p));
const sized = (p: Params) => `${Math.round(p.w)} × ${Math.round(p.h)} mm`;

export function frameAt(c: Clip, t: number, origin: Pt): Frame {
  const actAt = c.start + FLY;
  const holdAt = actAt + ACT[c.kind];
  if (t < actAt) {
    const u = easeOut(clamp01((t - c.start) / FLY));
    const g = grip(c, c.kind === "add" ? { ...c.to, w: 0, h: 0 } : c.from);
    return {
      cursor: { x: lerp(origin.x, g.x, u), y: lerp(origin.y, g.y, u) },
      box: null,
      badge: null,
      layer: null,
      hide: c.kind === "add",
      press: false,
      done: false,
    };
  }
  const u = easeInOut(clamp01((t - actAt) / ACT[c.kind]));
  const box = c.kind === "add" ? { ...c.to, w: c.to.w * u, h: c.to.h * u } : lerpParams(c.from, c.to, u);
  const hold = t >= holdAt;
  return {
    cursor: grip(c, box),
    box,
    badge: c.kind === "touch" ? null : sized(box),
    layer: c.kind === "move" && !hold ? box : null,
    hide: c.kind === "add" && !hold,
    press: c.kind === "touch" && !hold,
    done: t >= c.end,
  };
}

export const restingSpot = (slot: number, W: number, H: number): Pt => ({
  x: W * (0.2 + 0.2 * slot),
  y: H * 1.08,
});

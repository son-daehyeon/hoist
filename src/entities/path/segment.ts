import type { Anchor, Pt, Subpath } from "./model.ts";

export type Seg = [Pt, Pt, Pt, Pt];

const RAD = Math.PI / 180;

export const add = (a: Pt, b: Pt): Pt => ({ x: a.x + b.x, y: a.y + b.y });
export const sub = (a: Pt, b: Pt): Pt => ({ x: a.x - b.x, y: a.y - b.y });
export const mul = (a: Pt, k: number): Pt => ({ x: a.x * k, y: a.y * k });
export const dot = (a: Pt, b: Pt): number => a.x * b.x + a.y * b.y;
export const len = (a: Pt): number => Math.hypot(a.x, a.y);
export const lerp = (a: Pt, b: Pt, t: number): Pt => ({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });
export const same = (a: Pt, b: Pt): boolean => Math.abs(a.x - b.x) < 1e-9 && Math.abs(a.y - b.y) < 1e-9;
export const corner = (p: Pt): Anchor => ({ p, hIn: null, hOut: null, kind: "corner" });

export const segCount = (s: Subpath): number =>
  s.anchors.length < 2 ? 0 : s.closed ? s.anchors.length : s.anchors.length - 1;

export function segment(s: Subpath, i: number): Seg {
  const a = s.anchors[i];
  const b = s.anchors[(i + 1) % s.anchors.length];
  if (!a.hOut && !b.hIn) return [a.p, lerp(a.p, b.p, 1 / 3), lerp(a.p, b.p, 2 / 3), b.p];
  return [a.p, a.hOut ?? a.p, b.hIn ?? b.p, b.p];
}

export const segments = (s: Subpath): Seg[] => Array.from({ length: segCount(s) }, (_, i) => segment(s, i));

export function pointAt([p0, p1, p2, p3]: Seg, t: number): Pt {
  const u = 1 - t;
  const [a, b, c, d] = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
  return { x: a * p0.x + b * p1.x + c * p2.x + d * p3.x, y: a * p0.y + b * p1.y + c * p2.y + d * p3.y };
}

export function derivative([p0, p1, p2, p3]: Seg, t: number): Pt {
  const u = 1 - t;
  return add(add(mul(sub(p1, p0), 3 * u * u), mul(sub(p2, p1), 6 * u * t)), mul(sub(p3, p2), 3 * t * t));
}

export function second([p0, p1, p2, p3]: Seg, t: number): Pt {
  const a = add(sub(p2, mul(p1, 2)), p0);
  const b = add(sub(p3, mul(p2, 2)), p1);
  return mul(add(mul(a, 1 - t), mul(b, t)), 6);
}

export function split([p0, p1, p2, p3]: Seg, t: number): [Seg, Seg] {
  const [q0, q1, q2] = [lerp(p0, p1, t), lerp(p1, p2, t), lerp(p2, p3, t)];
  const [r0, r1] = [lerp(q0, q1, t), lerp(q1, q2, t)];
  const s = lerp(r0, r1, t);
  return [
    [p0, q0, r0, s],
    [s, r1, q2, p3],
  ];
}

function roots(a: number, b: number, c: number): number[] {
  if (Math.abs(a) < 1e-12) return Math.abs(b) < 1e-12 ? [] : [-c / b];
  const d = b * b - 4 * a * c;
  if (d < 0) return [];
  const r = Math.sqrt(d);
  return [(-b + r) / (2 * a), (-b - r) / (2 * a)];
}

export function extrema([p0, p1, p2, p3]: Seg): number[] {
  const ts: number[] = [];
  for (const k of ["x", "y"] as const) {
    const [a, b, c] = [p1[k] - p0[k], p2[k] - p1[k], p3[k] - p2[k]];
    ts.push(...roots(a - 2 * b + c, 2 * (b - a), a));
  }
  return ts.filter((t) => t > 0 && t < 1);
}

export function kindOf(a: Anchor): Anchor["kind"] {
  if (!a.hIn || !a.hOut) return "corner";
  const vIn = sub(a.p, a.hIn);
  const vOut = sub(a.hOut, a.p);
  if (len(vIn) < 1e-9 || len(vOut) < 1e-9) return "corner";
  const angle = Math.atan2(vIn.x * vOut.y - vIn.y * vOut.x, dot(vIn, vOut));
  return Math.abs(angle) < RAD ? "smooth" : "corner";
}

export function closeSub(s: Subpath): Subpath {
  const a = s.anchors;
  const n = a.length;
  if (n < 2 || !same(a[0].p, a[n - 1].p) || a[n - 1].hOut) return { ...s, closed: true };
  const first = { ...a[0], hIn: a[n - 1].hIn };
  return { closed: true, anchors: [{ ...first, kind: kindOf(first) }, ...a.slice(1, n - 1)] };
}

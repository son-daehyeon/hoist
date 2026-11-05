import type { Anchor, Path, Pt, Rect, Subpath } from "./model.ts";
import {
  add,
  closeSub,
  derivative,
  dot,
  extrema,
  len,
  mul,
  pointAt,
  second,
  segCount,
  segment,
  split,
  sub,
  type Seg,
} from "./segment.ts";

const SAMPLES = 16;
const NEWTON = 4;

function mapPoints(path: Path, f: (p: Pt) => Pt): Path {
  return {
    subpaths: path.subpaths.map((s) => ({
      closed: s.closed,
      anchors: s.anchors.map((a) => ({
        ...a,
        p: f(a.p),
        hIn: a.hIn && f(a.hIn),
        hOut: a.hOut && f(a.hOut),
      })),
    })),
  };
}

export function bbox(path: Path): Rect {
  const xs: number[] = [];
  const ys: number[] = [];
  const take = (p: Pt) => {
    xs.push(p.x);
    ys.push(p.y);
  };
  for (const s of path.subpaths) {
    for (const a of s.anchors) take(a.p);
    for (let i = 0; i < segCount(s); i++) {
      const seg = segment(s, i);
      for (const t of extrema(seg)) take(pointAt(seg, t));
    }
  }
  if (xs.length === 0) return { x: 0, y: 0, w: 0, h: 0 };
  const [x, y] = [Math.min(...xs), Math.min(...ys)];
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
}

export const translate = (path: Path, dx: number, dy: number): Path =>
  mapPoints(path, (p) => ({ x: p.x + dx, y: p.y + dy }));

export const scale = (path: Path, sx: number, sy: number, origin: Pt): Path =>
  mapPoints(path, (p) => ({ x: origin.x + (p.x - origin.x) * sx, y: origin.y + (p.y - origin.y) * sy }));

export function normalize(path: Path): { path: Path } & Rect {
  const b = bbox(path);
  return { path: translate(path, -b.x, -b.y), ...b };
}

function edit(path: Path, si: number, f: (s: Subpath) => void): Path {
  const copy = structuredClone(path);
  f(copy.subpaths[si]);
  return copy;
}

export const moveAnchor = (path: Path, si: number, ai: number, to: Pt): Path =>
  edit(path, si, (s) => {
    const a = s.anchors[ai];
    const d = sub(to, a.p);
    a.p = to;
    a.hIn = a.hIn && add(a.hIn, d);
    a.hOut = a.hOut && add(a.hOut, d);
  });

export const moveHandle = (path: Path, si: number, ai: number, side: "in" | "out", to: Pt): Path =>
  edit(path, si, (s) => {
    const a = s.anchors[ai];
    const other = side === "in" ? a.hOut : a.hIn;
    if (side === "in") a.hIn = to;
    else a.hOut = to;
    const v = sub(to, a.p);
    if (a.kind !== "smooth" || !other || len(v) < 1e-9) return;
    const mirrored = sub(a.p, mul(v, len(sub(other, a.p)) / len(v)));
    if (side === "in") a.hOut = mirrored;
    else a.hIn = mirrored;
  });

export const toggleKind = (path: Path, si: number, ai: number): Path =>
  edit(path, si, (s) => {
    const a = s.anchors[ai];
    if (a.kind === "smooth") {
      a.kind = "corner";
      return;
    }
    a.kind = "smooth";
    const n = s.anchors.length;
    const prev = ai > 0 ? s.anchors[ai - 1] : s.closed && n > 1 ? s.anchors[n - 1] : null;
    const next = ai < n - 1 ? s.anchors[ai + 1] : s.closed && n > 1 ? s.anchors[0] : null;
    const dir = sub(next?.p ?? a.p, prev?.p ?? a.p);
    const l = len(dir);
    if (l < 1e-9) return;
    const unit = mul(dir, 1 / l);
    a.hIn = prev && sub(a.p, mul(unit, len(sub(a.p, prev.p)) / 3));
    a.hOut = next && add(a.p, mul(unit, len(sub(next.p, a.p)) / 3));
  });

export function removeAnchor(path: Path, si: number, ai: number): Path {
  const copy = edit(path, si, (s) => s.anchors.splice(ai, 1));
  copy.subpaths = copy.subpaths.filter((s) => s.anchors.length > 0);
  return copy;
}

export const insertPoint = (path: Path, si: number, segIndex: number, t: number): Path =>
  edit(path, si, (s) => {
    const a = s.anchors[segIndex];
    const b = s.anchors[(segIndex + 1) % s.anchors.length];
    const [l, r] = split(segment(s, segIndex), t);
    const line = !a.hOut && !b.hIn;
    const anchor: Anchor = line
      ? { p: l[3], hIn: null, hOut: null, kind: "corner" }
      : { p: l[3], hIn: l[2], hOut: r[1], kind: "smooth" };
    if (!line) {
      a.hOut = l[1];
      b.hIn = r[2];
    }
    s.anchors.splice(segIndex + 1, 0, anchor);
  });

function nearestOnSeg(seg: Seg, p: Pt): { t: number; dist: number } {
  let best = { t: 0, dist: Infinity };
  const consider = (t: number) => {
    const dist = len(sub(pointAt(seg, t), p));
    if (dist < best.dist) best = { t, dist };
  };
  for (let i = 0; i <= SAMPLES; i++) consider(i / SAMPLES);
  let t = best.t;
  for (let k = 0; k < NEWTON; k++) {
    const d = sub(pointAt(seg, t), p);
    const d1 = derivative(seg, t);
    const f2 = dot(d1, d1) + dot(d, second(seg, t));
    if (Math.abs(f2) < 1e-12) break;
    t = Math.min(1, Math.max(0, t - dot(d, d1) / f2));
    consider(t);
  }
  return best;
}

export function nearestOnPath(
  path: Path,
  p: Pt,
  tolerance: number,
): { si: number; segIndex: number; t: number; dist: number; point: Pt } | null {
  let best: ReturnType<typeof nearestOnPath> = null;
  path.subpaths.forEach((s, si) => {
    for (let segIndex = 0; segIndex < segCount(s); segIndex++) {
      const seg = segment(s, segIndex);
      const { t, dist } = nearestOnSeg(seg, p);
      if (dist <= tolerance && (!best || dist < best.dist))
        best = { si, segIndex, t, dist, point: pointAt(seg, t) };
    }
  });
  return best;
}

export const closeSubpath = (path: Path, si: number): Path => ({
  subpaths: path.subpaths.map((s, i) => (i === si ? closeSub(s) : s)),
});

export function appendAnchor(path: Path, si: number, anchor: Anchor): Path {
  const copy = structuredClone(path);
  if (si === copy.subpaths.length) copy.subpaths.push({ closed: false, anchors: [] });
  copy.subpaths[si].anchors.push(anchor);
  return copy;
}

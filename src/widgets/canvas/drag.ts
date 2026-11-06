import type { Box } from "@/entities/layer";

export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export type Params = { x: number; y: number; w: number; h: number; deg: number };
export type Matrix = [number, number, number, number, number, number];
type Pt = { x: number; y: number };

const RAD = Math.PI / 180;

export const HANDLE_UV: Record<Handle, [number, number]> = {
  nw: [0, 0],
  n: [0.5, 0],
  ne: [1, 0],
  e: [1, 0.5],
  se: [1, 1],
  s: [0.5, 1],
  sw: [0, 1],
  w: [0, 0.5],
};

const OPPOSITE: Record<Handle, Handle> = {
  nw: "se",
  n: "s",
  ne: "sw",
  e: "w",
  se: "nw",
  s: "n",
  sw: "ne",
  w: "e",
};

export const centerOf = (p: Params): Pt => ({ x: p.x + p.w / 2, y: p.y + p.h / 2 });

export function rotatePoint(pt: Pt, c: Pt, deg: number): Pt {
  const [s, k] = [Math.sin(deg * RAD), Math.cos(deg * RAD)];
  const [dx, dy] = [pt.x - c.x, pt.y - c.y];
  return { x: c.x + dx * k - dy * s, y: c.y + dx * s + dy * k };
}

export const pointOn = (p: Params, u: number, v: number): Pt =>
  rotatePoint({ x: p.x + u * p.w, y: p.y + v * p.h }, centerOf(p), p.deg);

export function aabb(p: Params): Box {
  const pts = [pointOn(p, 0, 0), pointOn(p, 1, 0), pointOn(p, 1, 1), pointOn(p, 0, 1)];
  const xs = pts.map((q) => q.x);
  const ys = pts.map((q) => q.y);
  const [x, y] = [Math.min(...xs), Math.min(...ys)];
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
}

export function unionBox(boxes: Box[]): Box {
  const x = Math.min(...boxes.map((b) => b.x));
  const y = Math.min(...boxes.map((b) => b.y));
  return {
    x,
    y,
    w: Math.max(...boxes.map((b) => b.x + b.w)) - x,
    h: Math.max(...boxes.map((b) => b.y + b.h)) - y,
  };
}

export const moved = (p: Params, dx: number, dy: number): Params => ({ ...p, x: p.x + dx, y: p.y + dy });

export type Frac = [number, number, number, number];

export function inkFrame(p: Params, f: Frac | undefined): Params {
  if (!f) return p;
  const [w, h] = [f[2] * p.w, f[3] * p.h];
  const c = rotatePoint({ x: p.x + f[0] * p.w + w / 2, y: p.y + f[1] * p.h + h / 2 }, centerOf(p), p.deg);
  return { x: c.x - w / 2, y: c.y - h / 2, w, h, deg: p.deg };
}

export function rotatedAbout(p: Params, pivot: Pt, deg: number): Params {
  const c = rotatePoint(centerOf(p), pivot, deg);
  return { ...p, x: c.x - p.w / 2, y: c.y - p.h / 2, deg: p.deg + deg };
}

export function mapInto(p: Params, from: Box, to: Box): Params {
  const sx = to.w / (from.w || 1);
  const sy = to.h / (from.h || 1);
  const c = centerOf(p);
  const [w, h] = [p.w * sx, p.h * sy];
  return { x: to.x + (c.x - from.x) * sx - w / 2, y: to.y + (c.y - from.y) * sy - h / 2, w, h, deg: p.deg };
}

const ARROWS: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
};

export function nudged(box: Box, key: string, big: boolean): Box | null {
  const d = ARROWS[key];
  if (!d) return null;
  const step = big ? 10 : 1;
  return { ...box, x: box.x + d[0] * step, y: box.y + d[1] * step };
}

export function resize(b: Box, h: Handle, dx: number, dy: number): Box {
  const west = h.includes("w");
  const north = h.includes("n");
  let w = west ? b.w - dx : h.includes("e") ? b.w + dx : b.w;
  let hh = north ? b.h - dy : h.includes("s") ? b.h + dy : b.h;
  w = Math.max(1, w);
  hh = Math.max(1, hh);
  return { x: west ? b.x + b.w - w : b.x, y: north ? b.y + b.h - hh : b.y, w, h: hh };
}

export function withRatio(b0: Box, b: Box, h: Handle): Box {
  const r = b0.w / (b0.h || 1);
  const horiz = h.includes("e") || h.includes("w");
  const vert = h.includes("n") || h.includes("s");
  let w = b.w;
  let hh = b.h;
  if (horiz && (!vert || w / b0.w >= hh / b0.h)) hh = w / r;
  else w = hh * r;
  w = Math.max(1, w);
  hh = Math.max(1, hh);
  const x = h.includes("w") ? b0.x + b0.w - w : h.includes("e") ? b0.x : b0.x + (b0.w - w) / 2;
  const y = h.includes("n") ? b0.y + b0.h - hh : h.includes("s") ? b0.y : b0.y + (b0.h - hh) / 2;
  return { x, y, w, h: hh };
}

export const lockAxis = (dx: number, dy: number): [number, number] =>
  Math.abs(dx) >= Math.abs(dy) ? [dx, 0] : [0, dy];

type Pull = { pull: (axis: "x" | "y", value: number) => number; tolerance: number };

export function resizedFrame(
  f: Params,
  h: Handle,
  dx: number,
  dy: number,
  opts: { keepRatio: boolean; snap?: (b: Box) => Box; snapTurned?: Pull },
): Params {
  const local = rotatePoint({ x: dx, y: dy }, { x: 0, y: 0 }, -f.deg);
  const box0 = { x: f.x, y: f.y, w: f.w, h: f.h };
  const anchored = (b: Box): Params => {
    const [u, v] = HANDLE_UV[OPPOSITE[h]];
    const before = pointOn(f, u, v);
    const after = pointOn({ ...b, deg: f.deg }, u, v);
    return { ...b, x: b.x + before.x - after.x, y: b.y + before.y - after.y, deg: f.deg };
  };
  let b = resize(box0, h, local.x, local.y);
  if (opts.snap) b = opts.snap(b);
  else if (opts.snapTurned) {
    const extra = turnedSnap(f.deg, h, pointOn(anchored(b), ...HANDLE_UV[h]), opts.snapTurned);
    if (extra) b = resize(box0, h, local.x + extra.x, local.y + extra.y);
  }
  if (opts.keepRatio) b = withRatio(box0, b, h);
  return anchored(b);
}

function turnedSnap(deg: number, h: Handle, p: Pt, { pull, tolerance }: Pull): Pt | null {
  const [ox, oy] = [pull("x", p.x), pull("y", p.y)];
  if (!ox && !oy) return null;
  const sideways = h.includes("e") || h.includes("w");
  const upways = h.includes("n") || h.includes("s");
  if (sideways && upways) return rotatePoint({ x: ox, y: oy }, { x: 0, y: 0 }, -deg);
  const axis = sideways
    ? { x: Math.cos(deg * RAD), y: Math.sin(deg * RAD) }
    : { x: -Math.sin(deg * RAD), y: Math.cos(deg * RAD) };
  const steps = [
    ox && Math.abs(axis.x) > 1e-6 ? ox / axis.x : null,
    oy && Math.abs(axis.y) > 1e-6 ? oy / axis.y : null,
  ]
    .filter((t): t is number => t !== null && Math.abs(t) <= tolerance)
    .sort((a, b) => Math.abs(a) - Math.abs(b));
  if (!steps.length) return null;
  return sideways ? { x: steps[0], y: 0 } : { x: 0, y: steps[0] };
}

export const mul = (m: Matrix, n: Matrix): Matrix => [
  m[0] * n[0] + m[2] * n[1],
  m[1] * n[0] + m[3] * n[1],
  m[0] * n[2] + m[2] * n[3],
  m[1] * n[2] + m[3] * n[3],
  m[0] * n[4] + m[2] * n[5] + m[4],
  m[1] * n[4] + m[3] * n[5] + m[5],
];
export const IDENTITY: Matrix = [1, 0, 0, 1, 0, 0];
const translate = (x: number, y: number): Matrix => [1, 0, 0, 1, x, y];
function rotation(deg: number, c: Pt): Matrix {
  const [s, k] = [Math.sin(deg * RAD), Math.cos(deg * RAD)];
  return [translate(c.x, c.y), [k, s, -s, k, 0, 0] as Matrix, translate(-c.x, -c.y)].reduce(mul);
}

export function matrixFor(base: Params, next: Params): Matrix {
  return [
    rotation(next.deg, centerOf(next)),
    translate(next.x, next.y),
    [next.w / (base.w || 1), 0, 0, next.h / (base.h || 1), 0, 0] as Matrix,
    translate(-base.x, -base.y),
    rotation(-base.deg, centerOf(base)),
  ].reduce(mul);
}

export const applyMatrix = (m: Matrix, p: Pt): Pt => ({
  x: m[0] * p.x + m[2] * p.y + m[4],
  y: m[1] * p.x + m[3] * p.y + m[5],
});

export function invert(m: Matrix): Matrix {
  const det = m[0] * m[3] - m[1] * m[2];
  return [
    m[3] / det,
    -m[1] / det,
    -m[2] / det,
    m[0] / det,
    (m[2] * m[5] - m[3] * m[4]) / det,
    (m[1] * m[4] - m[0] * m[5]) / det,
  ];
}

const angleOf = (m: Matrix) => Math.atan2(m[1], m[0]) / RAD;

export const localMatrix = (l: Params): Matrix =>
  mul(translate(l.x, l.y), rotation(l.deg, { x: l.w / 2, y: l.h / 2 }));

export function placed(parent: Matrix, local: Params): Params {
  const c = applyMatrix(mul(parent, localMatrix(local)), { x: local.w / 2, y: local.h / 2 });
  return { ...local, x: c.x - local.w / 2, y: c.y - local.h / 2, deg: local.deg + angleOf(parent) };
}

export function unplaced(parent: Matrix, world: Params): Params {
  const c = applyMatrix(invert(parent), centerOf(world));
  return { ...world, x: c.x - world.w / 2, y: c.y - world.h / 2, deg: world.deg - angleOf(parent) };
}

export function drawn(a: Pt, b: Pt, o: { line: boolean; shift: boolean; thick: number }): Params {
  let dx = b.x - a.x;
  let dy = b.y - a.y;
  if (o.line) {
    const len = Math.hypot(dx, dy);
    const deg = o.shift ? Math.round(Math.atan2(dy, dx) / RAD / 45) * 45 : Math.atan2(dy, dx) / RAD;
    const c = { x: a.x + (Math.cos(deg * RAD) * len) / 2, y: a.y + (Math.sin(deg * RAD) * len) / 2 };
    return { x: c.x - len / 2, y: c.y - o.thick / 2, w: len, h: o.thick, deg };
  }
  if (o.shift) {
    const side = Math.max(Math.abs(dx), Math.abs(dy));
    dx = Math.sign(dx || 1) * side;
    dy = Math.sign(dy || 1) * side;
  }
  return { x: Math.min(a.x, a.x + dx), y: Math.min(a.y, a.y + dy), w: Math.abs(dx), h: Math.abs(dy), deg: 0 };
}

export function refit(l: Params, b: { x: number; y: number; w: number; h: number }): Params {
  const c = applyMatrix(localMatrix(l), { x: b.x + b.w / 2, y: b.y + b.h / 2 });
  return { x: c.x - b.w / 2, y: c.y - b.h / 2, w: b.w, h: b.h, deg: l.deg };
}

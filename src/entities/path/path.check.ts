import {
  appendAnchor,
  bbox,
  closeSubpath,
  insertPoint,
  moveAnchor,
  moveHandle,
  nearestOnPath,
  normalize,
  parsePath,
  pointAt,
  removeAnchor,
  scale,
  segments,
  serializePath,
  toggleKind,
  translate,
  type Path,
  type Pt,
} from "./index.ts";

const assert = {
  ok(cond: unknown, msg?: string) {
    if (!cond) throw new Error(msg ?? "assertion failed");
  },
  equal(actual: unknown, expected: unknown) {
    const [a, b] = [JSON.stringify(actual), JSON.stringify(expected)];
    if (a !== b) throw new Error(`${a} !== ${b}`);
  },
  throws(f: () => void) {
    try {
      f();
    } catch {
      return;
    }
    throw new Error("expected an error");
  },
};

const near = (a: Pt, b: Pt, eps = 0.01) =>
  assert.ok(Math.hypot(a.x - b.x, a.y - b.y) <= eps, `${JSON.stringify(a)} vs ${JSON.stringify(b)}`);
const samples = (path: Path, n = 20): Pt[] =>
  path.subpaths.flatMap((s) =>
    segments(s).flatMap((seg) => Array.from({ length: n }, (_, i) => pointAt(seg, i / n))),
  );
const sameShape = (a: Path, b: Path) => {
  const [sa, sb] = [samples(a), samples(b)];
  assert.equal(sa.length, sb.length);
  sa.forEach((p, i) => near(p, sb[i]));
};

const CIRCLE =
  "M50 0 C77.61 0 100 22.39 100 50 C100 77.61 77.61 100 50 100 C22.39 100 0 77.61 0 50 C0 22.39 22.39 0 50 0 Z";
const COMPLEX = "M10 10 L90 10 C110 30 110 70 90 90 L10 90 Z M20 20 C30 10 50 10 60 20 L60 60 M70 70 L80 80";

assert.equal(serializePath(parsePath(CIRCLE)), CIRCLE);
assert.equal(serializePath(parsePath(COMPLEX)), COMPLEX);
const circle = parsePath(CIRCLE);
assert.equal(circle.subpaths.length, 1);
assert.equal(circle.subpaths[0].anchors.length, 4);
assert.ok(circle.subpaths[0].closed);
assert.ok(circle.subpaths[0].anchors.every((a) => a.kind === "smooth"));
assert.ok(parsePath(COMPLEX).subpaths[0].anchors.every((a) => a.kind === "corner"));
assert.equal(serializePath(parsePath("M0 0 L10 0 L10 10 L0 0 Z")), "M0 0 L10 0 L10 10 Z");
assert.equal(serializePath(parsePath("M1e1 1E1 L2.5e1 1e1")), "M10 10 L25 10");
assert.equal(serializePath(parsePath("M0,0l10-5.5.5-1")), "M0 0 L10 -5.5 L10.5 -6.5");
assert.equal(serializePath(parsePath("M0 0 L10 0 Z L5 5")), "M0 0 L10 0 Z M0 0 L5 5");
assert.equal(serializePath(parsePath("M3 4")), "M3 4");
assert.throws(() => parsePath("M0 0 A5 5 0 0 1 10 10"));
assert.throws(() => parsePath("M0 0 L10"));

sameShape(parsePath("m10 10 h20 v20 h-20 z"), parsePath("M10 10 L30 10 L30 30 L10 30 Z"));
sameShape(
  parsePath("m0 0 c10 20 30 20 40 0 s30 -20 40 0"),
  parsePath("M0 0 C10 20 30 20 40 0 C50 -20 70 -20 80 0"),
);
sameShape(
  parsePath("M0 0 Q20 40 40 0 T80 0"),
  parsePath("M0 0 C13.3333 26.6667 26.6667 26.6667 40 0 C53.3333 -26.6667 66.6667 -26.6667 80 0"),
);
sameShape(parsePath("M0 0 S10 10 20 0"), parsePath("M0 0 C0 0 10 10 20 0"));
sameShape(parsePath("M0 0 L10 0 C10 10 20 10 20 0"), parsePath("M0 0 L10 0 C10 10 20 10 20 0"));

const s = parsePath("M0 0 C0 100 100 -100 100 0 L50 50 Z");
for (const [segIndex, t] of [
  [0, 0.3],
  [1, 0.5],
  [2, 0.7],
] as const) {
  const after = insertPoint(s, 0, segIndex, t);
  assert.equal(after.subpaths[0].anchors.length, 4);
  const before = segments(s.subpaths[0])[segIndex];
  const [l, r] = [segments(after.subpaths[0])[segIndex], segments(after.subpaths[0])[segIndex + 1]];
  for (let i = 0; i <= 20; i++) {
    const u = i / 20;
    near(pointAt(before, u), u < t ? pointAt(l, u / t) : pointAt(r, (u - t) / (1 - t)), 1e-9);
  }
}

const brute = (path: Path) => {
  const pts = samples(path, 2000);
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  return {
    x: Math.min(...xs),
    y: Math.min(...ys),
    w: Math.max(...xs) - Math.min(...xs),
    h: Math.max(...ys) - Math.min(...ys),
  };
};
const wave = parsePath("M0 0 C0 100 100 -100 100 0");
const wb = bbox(wave);
const bb = brute(wave);
assert.ok(Math.abs(wb.h - bb.h) < 0.01 && Math.abs(wb.y - bb.y) < 0.01, JSON.stringify([wb, bb]));
assert.ok(wb.h > 50 && wb.h < 100);
const cb = bbox(circle);
assert.ok(
  [cb.x, cb.y, cb.w, cb.h].every((v, i) => Math.abs(v - [0, 0, 100, 100][i]) < 1e-9),
  JSON.stringify(cb),
);
assert.equal(bbox({ subpaths: [] }), { x: 0, y: 0, w: 0, h: 0 });

const shifted = translate(circle, 7, -3);
const nb = normalize(shifted);
assert.ok(Math.abs(nb.x - 7) < 1e-9 && Math.abs(nb.y + 3) < 1e-9 && Math.abs(nb.w - 100) < 1e-9);
const nbb = bbox(nb.path);
assert.ok(Math.abs(nbb.x) < 1e-9 && Math.abs(nbb.y) < 1e-9);
const half = bbox(scale(circle, 0.5, 2, { x: 0, y: 0 }));
assert.ok(Math.abs(half.w - 50) < 1e-9 && Math.abs(half.h - 200) < 1e-9);

const moved = moveAnchor(circle, 0, 0, { x: 60, y: 10 });
near(moved.subpaths[0].anchors[0].hOut!, { x: 87.61, y: 10 });
near(moved.subpaths[0].anchors[0].hIn!, { x: 32.39, y: 10 });
assert.equal(circle.subpaths[0].anchors[0].p.x, 50);

const bent = moveHandle(circle, 0, 0, "out", { x: 50, y: 20 });
near(bent.subpaths[0].anchors[0].hOut!, { x: 50, y: 20 });
near(bent.subpaths[0].anchors[0].hIn!, { x: 50, y: -27.61 });
const cornered = toggleKind(circle, 0, 0);
assert.equal(cornered.subpaths[0].anchors[0].kind, "corner");
const bentCorner = moveHandle(cornered, 0, 0, "out", { x: 50, y: 20 });
near(bentCorner.subpaths[0].anchors[0].hIn!, { x: 22.39, y: 0 });

const square = parsePath("M0 0 L100 0 L100 100 L0 100 Z");
const smoothed = toggleKind(square, 0, 1);
const a1 = smoothed.subpaths[0].anchors[1];
assert.equal(a1.kind, "smooth");
near(a1.hIn!, { x: 100 - 100 / 3 / Math.SQRT2, y: -100 / 3 / Math.SQRT2 });
near(a1.hOut!, { x: 100 + 100 / 3 / Math.SQRT2, y: 100 / 3 / Math.SQRT2 });
const openEnd = toggleKind(parsePath("M0 0 L30 0 L30 30"), 0, 2);
assert.equal(openEnd.subpaths[0].anchors[2].hOut, null);
near(openEnd.subpaths[0].anchors[2].hIn!, { x: 30, y: 20 });

const fewer = removeAnchor(circle, 0, 1);
assert.ok(fewer.subpaths[0].closed);
assert.equal(fewer.subpaths[0].anchors.length, 3);
assert.equal(serializePath(removeAnchor(parsePath("M1 1"), 0, 0)), "");

const hit = nearestOnPath(circle, { x: 150, y: 50 }, 100)!;
near(hit.point, { x: 100, y: 50 }, 1e-3);
assert.ok(Math.abs(hit.dist - 50) < 1e-3);
assert.equal(hit.si, 0);
const top = nearestOnPath(circle, { x: 50, y: -7 }, 10)!;
near(top.point, { x: 50, y: 0 }, 1e-3);
assert.equal(nearestOnPath(circle, { x: 50, y: -7 }, 5), null);
const onLine = nearestOnPath(square, { x: 40, y: 3 }, 5)!;
near(onLine.point, { x: 40, y: 0 }, 1e-6);
assert.equal(onLine.segIndex, 0);
assert.ok(Math.abs(onLine.t - 0.4) < 1e-6);
for (let i = 0; i <= 100; i++) {
  const t = i / 100;
  const onWave = nearestOnPath(wave, pointAt(segments(wave.subpaths[0])[0], t), 0.01)!;
  assert.ok(Math.abs(onWave.t - t) < 1e-3 && onWave.dist < 1e-3, `t=${t} → ${JSON.stringify(onWave)}`);
}

const pen = appendAnchor({ subpaths: [] }, 0, { p: { x: 0, y: 0 }, hIn: null, hOut: null, kind: "corner" });
const pen2 = appendAnchor(
  appendAnchor(pen, 0, { p: { x: 10, y: 0 }, hIn: null, hOut: null, kind: "corner" }),
  0,
  {
    p: { x: 10, y: 10 },
    hIn: null,
    hOut: null,
    kind: "corner",
  },
);
assert.equal(serializePath(pen2), "M0 0 L10 0 L10 10");
assert.equal(serializePath(closeSubpath(pen2, 0)), "M0 0 L10 0 L10 10 Z");
assert.equal(
  serializePath(
    closeSubpath(
      appendAnchor(pen2, 0, { p: { x: 0, y: 0 }, hIn: { x: 5, y: 10 }, hOut: null, kind: "corner" }),
      0,
    ),
  ),
  "M0 0 L10 0 L10 10 C10 10 5 10 0 0 Z",
);

console.log("path: ok");

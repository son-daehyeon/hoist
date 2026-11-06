import {
  aabb,
  applyMatrix,
  drawn,
  inkFrame,
  lockAxis,
  mapInto,
  matrixFor,
  moved,
  nudged,
  localMatrix,
  placed,
  pointOn,
  refit,
  resizedFrame,
  rotatedAbout,
  unplaced,
  withRatio,
} from "./drag.ts";

const same = (actual: unknown, expected: unknown) => {
  const fix = (v: unknown) =>
    JSON.stringify(v, (_, x) => (typeof x === "number" ? Math.round(x * 1e6) / 1e6 : x));
  const [a, b] = [fix(actual), fix(expected)];
  if (a !== b) throw new Error(`${a} !== ${b}`);
};

const layer = { x: 10, y: 20, w: 40, h: 10 };
const p = { ...layer, deg: 0 };

same(nudged(layer, "ArrowRight", false), { ...layer, x: 11 });
same(nudged(layer, "ArrowUp", true), { ...layer, y: 10 });
same(nudged(layer, "Enter", false), null);
same(lockAxis(5, -2), [5, 0]);
same(lockAxis(1, 3), [0, 3]);
const b0 = { x: 0, y: 0, w: 40, h: 20 };
same(withRatio(b0, { x: 0, y: 0, w: 60, h: 22 }, "se"), { x: 0, y: 0, w: 60, h: 30 });
same(withRatio(b0, { x: 0, y: 0, w: 40, h: 30 }, "s"), { x: -10, y: 0, w: 60, h: 30 });

same(matrixFor(p, p), [1, 0, 0, 1, 0, 0]);
same(matrixFor(p, moved(p, 5, -3)), [1, 0, 0, 1, 5, -3]);

const turned = rotatedAbout(p, { x: 30, y: 25 }, 90);
same(turned, { x: 10, y: 20, w: 40, h: 10, deg: 90 });
same(applyMatrix(matrixFor(p, turned), { x: 10, y: 20 }), pointOn(turned, 0, 0));
same(aabb(turned), { x: 25, y: 5, w: 10, h: 40 });

const tilted = { x: 0, y: 0, w: 40, h: 20, deg: 30 };
const grown = resizedFrame(tilted, "e", 10, 0, { keepRatio: false });
same(pointOn(grown, 0, 0.5), pointOn(tilted, 0, 0.5));
same([grown.w > 40, grown.h, grown.deg], [true, 20, 30]);

same(mapInto(p, { x: 0, y: 0, w: 100, h: 100 }, { x: 0, y: 0, w: 200, h: 100 }), {
  x: 20,
  y: 20,
  w: 80,
  h: 10,
  deg: 0,
});

const group = localMatrix({ x: 100, y: 50, w: 200, h: 100, deg: 90 });
const child = { x: 10, y: 20, w: 40, h: 10, deg: 0 };
const world = placed(group, child);
same(world, { x: 205, y: 25, w: 40, h: 10, deg: 90 });
same(unplaced(group, world), child);
same(unplaced(group, moved(world, 0, 7)), { ...child, x: 17 });

const rect = { line: false, shift: false, thick: 2 };
same(drawn({ x: 10, y: 10 }, { x: 0, y: -5 }, rect), { x: 0, y: -5, w: 10, h: 15, deg: 0 });
same(drawn({ x: 10, y: 10 }, { x: 40, y: 20 }, { ...rect, shift: true }), {
  x: 10,
  y: 10,
  w: 30,
  h: 30,
  deg: 0,
});
same(drawn({ x: 10, y: 10 }, { x: 0, y: 30 }, { ...rect, shift: true }), {
  x: -10,
  y: 10,
  w: 20,
  h: 20,
  deg: 0,
});
const line = { line: true, shift: false, thick: 2 };
const seg = drawn({ x: 3, y: 4 }, { x: 13, y: 24 }, line);
same([pointOn(seg, 0, 0.5), pointOn(seg, 1, 0.5), seg.h], [{ x: 3, y: 4 }, { x: 13, y: 24 }, 2]);
same(drawn({ x: 0, y: 0 }, { x: 0, y: 10 }, line), { x: -5, y: 4, w: 10, h: 2, deg: 90 });
same(drawn({ x: 0, y: 0 }, { x: 10, y: 9 }, { ...line, shift: true }).deg, 45);

same(refit({ x: 100, y: 50, w: 200, h: 100, deg: 0 }, { x: 10, y: 20, w: 40, h: 10 }), {
  x: 110,
  y: 70,
  w: 40,
  h: 10,
  deg: 0,
});
same(refit({ x: 100, y: 50, w: 200, h: 100, deg: 90 }, { x: 10, y: 20, w: 40, h: 10 }), {
  x: 205,
  y: 25,
  w: 40,
  h: 10,
  deg: 90,
});

same(inkFrame(p, undefined), p);
same(inkFrame(p, [0.1, 0.2, 0.5, 0.5]), { x: 14, y: 22, w: 20, h: 5, deg: 0 });
same(inkFrame(moved(p, 3, 4), [0.1, 0.2, 0.5, 0.5]), { x: 17, y: 26, w: 20, h: 5, deg: 0 });
const inkTurned = inkFrame({ ...p, deg: 90 }, [0.1, 0.2, 0.5, 0.5]);
same(pointOn(inkTurned, 0, 0), pointOn({ ...p, deg: 90 }, 0.1, 0.2));
same(pointOn(inkTurned, 1, 1), pointOn({ ...p, deg: 90 }, 0.6, 0.7));

const upright = { x: 0, y: 0, w: 100, h: 50, deg: 90 };
const toLine = (lines: number[]) => ({
  pull: (axis: "x" | "y", v: number) =>
    axis === "y" ? (lines.find((l) => Math.abs(l - v) <= 5) ?? v) - v : 0,
  tolerance: 5,
});
const snapped = resizedFrame(upright, "e", 0, 3, { keepRatio: false, snapTurned: toLine([80]) });
same([pointOn(snapped, 1, 0.5).y, snapped.w, pointOn(snapped, 0, 0.5)], [80, 105, pointOn(upright, 0, 0.5)]);
const corner = resizedFrame(upright, "se", -2, 3, { keepRatio: false, snapTurned: toLine([80]) });
same(pointOn(corner, 1, 1).y, 80);
same(resizedFrame(upright, "e", 0, 3, { keepRatio: false, snapTurned: toLine([200]) }).w, 103);

console.log("drag: ok");

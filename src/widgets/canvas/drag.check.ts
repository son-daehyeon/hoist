import {
  aabb,
  applyMatrix,
  lockAxis,
  mapInto,
  matrixFor,
  moved,
  nudged,
  localMatrix,
  placed,
  pointOn,
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

console.log("drag: ok");

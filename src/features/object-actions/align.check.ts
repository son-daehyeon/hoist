import { aabb, alignMoves, distributeMoves, union } from "./align.ts";

const same = (actual: unknown, expected: unknown) => {
  const round = (v: unknown) =>
    JSON.parse(JSON.stringify(v, (_, x) => (typeof x === "number" ? Math.round(x * 1000) / 1000 : x)));
  const [a, b] = [JSON.stringify(round(actual)), JSON.stringify(round(expected))];
  if (a !== b) throw new Error(`${a} !== ${b}`);
};

same(aabb({ x: 0, y: 0, w: 100, h: 20 }, 0), { x: 0, y: 0, w: 100, h: 20 });
same(aabb({ x: 0, y: 0, w: 100, h: 20 }, 90), { x: 40, y: -40, w: 20, h: 100 });
same(aabb({ x: 0, y: 0, w: 10, h: 10 }, 45).w, 10 * Math.SQRT2);

const items = [
  { id: "a", box: { x: 10, y: 0, w: 20, h: 10 } },
  { id: "b", box: { x: 50, y: 30, w: 40, h: 20 } },
];
const ref = union(items.map((i) => i.box));
same(ref, { x: 10, y: 0, w: 80, h: 50 });
same(alignMoves(items, ref, "left"), [
  { id: "a", dx: 0, dy: 0 },
  { id: "b", dx: -40, dy: 0 },
]);
same(alignMoves(items, ref, "middle"), [
  { id: "a", dx: 0, dy: 20 },
  { id: "b", dx: 0, dy: -15 },
]);
same(alignMoves([items[0]], { x: 0, y: 0, w: 100, h: 100 }, "right"), [{ id: "a", dx: 70, dy: 0 }]);

same(
  distributeMoves(
    [
      { id: "c", box: { x: 100, y: 0, w: 10, h: 10 } },
      { id: "a", box: { x: 0, y: 0, w: 10, h: 10 } },
      { id: "b", box: { x: 20, y: 0, w: 30, h: 10 } },
    ],
    "x",
  ),
  [
    { id: "a", dx: 0, dy: 0 },
    { id: "b", dx: 20, dy: 0 },
    { id: "c", dx: 0, dy: 0 },
  ],
);

console.log("align: ok");

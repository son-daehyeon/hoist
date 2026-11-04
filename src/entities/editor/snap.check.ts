import { guidesFor, snapLines, snapMove } from "./snap.ts";

const same = (actual: unknown, expected: unknown) => {
  const [a, b] = [JSON.stringify(actual), JSON.stringify(expected)];
  if (a !== b) throw new Error(`${a} !== ${b}`);
};

const lines = snapLines(200, 100, [{ x: 150, y: 70, w: 30, h: 20 }]);

const edge = snapMove({ x: 3, y: 30, w: 20, h: 10 }, lines, 6);
same(edge, { x: 0, y: 30, w: 20, h: 10 });
same(guidesFor(edge, lines), [{ axis: "x", at: 0 }]);

const center = snapMove({ x: 88.5, y: 20, w: 20, h: 10 }, lines, 6);
same(center, { x: 90, y: 20, w: 20, h: 10 });
same(guidesFor(center, lines), [{ axis: "x", at: 100 }]);

console.log("snap: ok");

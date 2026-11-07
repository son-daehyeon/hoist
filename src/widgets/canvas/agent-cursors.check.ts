import type { Layer } from "@/shared/api";
import { diffLayers, frameAt, planBatch, SLOTS, type Change } from "./agent-cursors.ts";

const same = (actual: unknown, expected: unknown) => {
  const fix = (v: unknown) =>
    JSON.stringify(v, (_, x) => (typeof x === "number" ? Math.round(x * 1e6) / 1e6 : x));
  const [a, b] = [fix(actual), fix(expected)];
  if (a !== b) throw new Error(`${a} !== ${b}`);
};

const layer = (id: string, type: "text" | "image" | "group" | "path", x: number, parent?: string): Layer =>
  ({ id, name: id, parent: parent ?? null, kind: { type }, x_mm: x, y_mm: 0, w_mm: 40, h_mm: 10 }) as Layer;
const at = (_: Layer[], l: Layer) => ({ x: l.x_mm, y: l.y_mm, w: l.w_mm, h: l.h_mm, deg: 0 });

const a = layer("a", "text", 0);
const b = layer("b", "image", 10);
const g = layer("g", "group", 20);
const child = layer("c", "path", 20, "g");
const prev = [a, b, g, child];
const next = [
  { ...a, x_mm: 5 },
  { ...b, name: "renamed" },
  { ...g, x_mm: 30 },
  { ...child, x_mm: 30 },
  layer("n", "path", 0),
];
const changes = diffLayers(prev, next, at);
same(
  changes.map((c) => [c.kind, c.id, c.work]),
  [
    ["move", "a", "text"],
    ["touch", "b", "image"],
    ["move", "g", "group"],
    ["add", "n", "shape"],
  ],
);
same(changes[0].from.x, 0);
same(changes[0].to.x, 5);
same(diffLayers(prev, prev, at), []);
same(
  diffLayers(prev, [a], at).map((c) => [c.kind, c.id]),
  [
    ["touch", "b"],
    ["touch", "g"],
  ],
);

const change = (id: string): Change => ({
  kind: "add",
  id,
  work: "text",
  from: { x: 0, y: 0, w: 100, h: 50, deg: 0 },
  to: { x: 0, y: 0, w: 100, h: 50, deg: 0 },
});
const many = Array.from({ length: 20 }, (_, i) => change(`l${i}`));
const plan = planBatch(
  many,
  1000,
  Array.from({ length: SLOTS }, () => 0),
);
same(plan.clips.length, many.length);
same(
  plan.clips.slice(0, SLOTS).map((c) => [c.slot, c.start]),
  [
    [0, 1000],
    [1, 1350],
    [2, 1700],
    [3, 2050],
  ],
);
same(plan.clips[4].slot, 0);
same(plan.clips[4].start, plan.clips[0].end);
same(plan.free.length, SLOTS);
same(Math.max(...plan.free), Math.max(...plan.clips.map((c) => c.end)));

const clip = planBatch([change("x")], 0, [0, 0, 0, 0]).clips[0];
const origin = { x: 300, y: 300 };
same(frameAt(clip, 0, origin).cursor, origin);
const flown = frameAt(clip, 650, origin);
same([flown.cursor, flown.box, flown.hide], [{ x: 0, y: 0 }, { x: 0, y: 0, w: 0, h: 0, deg: 0 }, true]);
const mid = frameAt(clip, 650 + 900, origin);
same([mid.box?.w, mid.box?.h, mid.badge, mid.cursor], [50, 25, "50 × 25 mm", { x: 50, y: 25 }]);
const held = frameAt(clip, 650 + 1800, origin);
same([held.box, held.hide, held.done], [clip.to, false, false]);
same(frameAt(clip, clip.end, origin).done, true);

const mv = planBatch(
  [
    {
      kind: "move",
      id: "m",
      work: "image",
      from: { x: 0, y: 0, w: 10, h: 10, deg: 0 },
      to: { x: 20, y: 0, w: 10, h: 10, deg: 0 },
    },
  ],
  0,
  [0, 0, 0, 0],
).clips[0];
const half = frameAt(mv, 650 + 800, origin);
same([half.layer, half.cursor, half.hide], [{ x: 10, y: 0, w: 10, h: 10, deg: 0 }, { x: 15, y: 5 }, false]);
same(frameAt(mv, 650 + 1600, origin).layer, null);

const tc = planBatch(
  [
    {
      kind: "touch",
      id: "t",
      work: "text",
      from: { x: 0, y: 0, w: 10, h: 10, deg: 0 },
      to: { x: 0, y: 0, w: 10, h: 10, deg: 0 },
    },
  ],
  0,
  [0, 0, 0, 0],
).clips[0];
same([frameAt(tc, 750, origin).press, frameAt(tc, 750, origin).badge], [true, null]);
same(frameAt(tc, 650 + 900, origin).press, false);

console.log("agent-cursors: ok");

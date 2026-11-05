import { reorder } from "./arrange.ts";

const same = (actual: unknown, expected: unknown) => {
  if (JSON.stringify(actual) !== JSON.stringify(expected))
    throw new Error(`${JSON.stringify(actual)} != ${JSON.stringify(expected)}`);
};

const order = ["a", "B", "c", "D", "e"];
const picked = new Set(["B", "D"]);
same(reorder(order, picked, "forward").order, ["a", "c", "B", "e", "D"]);
same(reorder(order, picked, "backward").order, ["B", "a", "D", "c", "e"]);
same(reorder(order, picked, "front").order, ["a", "c", "e", "B", "D"]);
same(reorder(order, picked, "back").order, ["B", "D", "a", "c", "e"]);
same(reorder(["a", "B", "C"], new Set(["B", "C"]), "forward").moves, []);
same(reorder(["A", "b"], new Set(["A"]), "back").moves, []);
same(reorder(["a", "B", "c"], new Set(["B"]), "forward").moves, [{ id: "B", index: 2 }]);

const replay = (start: string[], moves: { id: string; index: number }[]) =>
  moves.reduce((o, m) => {
    const rest = o.filter((x) => x !== m.id);
    rest.splice(m.index, 0, m.id);
    return rest;
  }, start);
for (const dir of ["forward", "backward", "front", "back"] as const) {
  const r = reorder(order, picked, dir);
  same(replay(order, r.moves), r.order);
}

console.log("arrange: ok");

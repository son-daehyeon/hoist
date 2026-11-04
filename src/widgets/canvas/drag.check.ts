import { lockAxis, nudged, rasterShift, shiftCss, withRatio } from "./drag.ts";

const same = (actual: unknown, expected: unknown) => {
  const [a, b] = [JSON.stringify(actual), JSON.stringify(expected)];
  if (a !== b) throw new Error(`${a} !== ${b}`);
};

const layer = { x: 10, y: 20, w: 40, h: 10 };
const raster = { x: 8, y: 18, w: 44, h: 14 };

same(rasterShift(raster, layer, { ...layer, x: 15, y: 25 }, 2), { dx: 10, dy: 10, sx: 1, sy: 1 });

const grown = rasterShift(raster, layer, { x: 10, y: 20, w: 80, h: 20 }, 1);
same(grown, { dx: -2, dy: -2, sx: 2, sy: 2 });
same(shiftCss(grown), "translate(-2px, -2px) scale(2, 2)");

same(nudged(layer, "ArrowRight", false), { ...layer, x: 11 });
same(nudged(layer, "ArrowUp", true), { ...layer, y: 10 });
same(nudged(layer, "Enter", false), null);

same(lockAxis(5, -2), [5, 0]);
same(lockAxis(1, 3), [0, 3]);

const b0 = { x: 0, y: 0, w: 40, h: 20 };
same(withRatio(b0, { x: 0, y: 0, w: 60, h: 22 }, "se"), { x: 0, y: 0, w: 60, h: 30 });
same(withRatio(b0, { x: 0, y: 0, w: 40, h: 30 }, "s"), { x: -10, y: 0, w: 60, h: 30 });

console.log("drag: ok");

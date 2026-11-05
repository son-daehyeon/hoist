import { cropBox, cropFrame, cropMap, cropPx, dragEdges, edgesOf } from "./crop.ts";

const same = (actual: unknown, expected: unknown) => {
  const fix = (v: unknown) =>
    JSON.stringify(v, (_, x) => (typeof x === "number" ? Math.round(x * 1e6) / 1e6 : x));
  if (fix(actual) !== fix(expected)) throw new Error(`${fix(actual)} != ${fix(expected)}`);
};

const full = { natural_w_px: 512, natural_h_px: 512, fit: "contain" as const, crop_px: null };
same(edgesOf(full), [0, 0, 512, 512]);
same(cropBox(edgesOf(full), cropMap(full, 660, 540)), { x: 60, y: 0, w: 540, h: 540 });

const cut = {
  ...full,
  fit: "fill" as const,
  crop_px: [100, 100, 200, 100] as [number, number, number, number],
};
same(edgesOf(cut), [100, 100, 300, 200]);
same(cropMap(cut, 400, 100), { sx: 2, sy: 1, ox: -200, oy: -100 });
same(cropBox(edgesOf(cut), cropMap(cut, 400, 100)), { x: 0, y: 0, w: 400, h: 100 });

same(dragEdges([100, 100, 300, 200], "nw", -500, 20, 512, 512), [0, 120, 300, 200]);
same(dragEdges([100, 100, 300, 200], "e", 1000, 0, 512, 512), [100, 100, 512, 200]);
same(dragEdges([100, 100, 300, 200], "w", 500, 0, 512, 512), [299, 100, 300, 200]);
same(dragEdges([100, 100, 300, 200], "move", 400, -400, 512, 512), [312, 0, 512, 100]);

same(cropPx([0, 0, 512, 512], 512, 512), null);
same(cropPx([10.4, 0, 300.6, 512], 512, 512), [10, 0, 291, 512]);

same(cropFrame({ x: 0, y: 0, w: 100, h: 50, deg: 90 }, { x: 0, y: 0, w: 50, h: 50 }), {
  x: 25,
  y: -25,
  w: 50,
  h: 50,
  deg: 90,
});

console.log("crop: ok");

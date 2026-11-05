import { hexToHsv, hsvToHex, mixHex, normalizeHex, paintCss } from "./color.ts";

const same = (actual: unknown, expected: unknown) => {
  const [a, b] = [JSON.stringify(actual), JSON.stringify(expected)];
  if (a !== b) throw new Error(`${a} !== ${b}`);
};

same(normalizeHex("1e3f7a"), "#1E3F7A");
same(normalizeHex("#abc"), "#AABBCC");
same(normalizeHex("#12345"), null);

for (const hex of ["#000000", "#FFFFFF", "#1E3F7A", "#D92D20", "#7F7F7F", "#00FF80"]) {
  same(hsvToHex(hexToHsv(hex)), hex);
}
same(hexToHsv("#FF0000"), { h: 0, s: 1, v: 1 });
same(Math.round(hexToHsv("#0000FF").h), 240);

same(mixHex("#000000", "#FFFFFF", 0.5), "#808080");

same(paintCss({ type: "solid", color: "#1E3F7A", opacity: 0.5 }), "rgba(30, 63, 122, 0.5)");
same(
  paintCss({
    type: "linear_gradient",
    angle_deg: 90,
    stops: [
      { offset: 1, color: "#FFFFFF", opacity: 1 },
      { offset: 0, color: "#000000", opacity: 1 },
    ],
  }),
  "linear-gradient(180deg, rgba(0, 0, 0, 1) 0%, rgba(255, 255, 255, 1) 100%)",
);

console.log("color: ok");

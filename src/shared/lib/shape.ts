type Point = [number, number];

const count = (n: number) => Math.min(64, Math.max(3, Math.round(n)));
const around = (w: number, h: number, steps: number, radius: (i: number) => number): Point[] =>
  Array.from({ length: steps }, (_, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / steps;
    const r = radius(i);
    return [w / 2 + (w / 2) * r * Math.cos(a), h / 2 + (h / 2) * r * Math.sin(a)];
  });

export const polygonPoints = (w: number, h: number, sides: number) => around(w, h, count(sides), () => 1);

export function starPoints(w: number, h: number, points: number, innerRatio: number) {
  const inner = Math.min(0.95, Math.max(0.05, innerRatio));
  return around(w, h, count(points) * 2, (i) => (i % 2 ? inner : 1));
}

export const closedPath = (points: Point[]) =>
  `${points.map(([x, y], i) => `${i ? "L" : "M"}${Math.round(x * 1000) / 1000} ${Math.round(y * 1000) / 1000}`).join("")}Z`;

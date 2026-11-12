import type { Paint } from "../api";

type Rgb = { r: number; g: number; b: number };
export type Hsv = { h: number; s: number; v: number };

export const RECENT_COLORS_KEY = "hoist.recent-colors";

export const solid = (color: string, opacity = 1): Paint => ({ type: "solid", color, opacity });

export function normalizeHex(s: string): string | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  return `#${h.toUpperCase()}`;
}

function hexToRgb(hex: string): Rgb {
  const n = parseInt((normalizeHex(hex) ?? "#000000").slice(1), 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

export const rgbToHex = ({ r, g, b }: Rgb) =>
  `#${[r, g, b]
    .map((v) =>
      Math.round(Math.min(255, Math.max(0, v)))
        .toString(16)
        .padStart(2, "0"),
    )
    .join("")}`.toUpperCase();

function rgbToHsv({ r, g, b }: Rgb): Hsv {
  const [R, G, B] = [r / 255, g / 255, b / 255];
  const max = Math.max(R, G, B);
  const d = max - Math.min(R, G, B);
  let h = 0;
  if (d > 0) h = max === R ? ((G - B) / d) % 6 : max === G ? (B - R) / d + 2 : (R - G) / d + 4;
  return { h: (h * 60 + 360) % 360, s: max === 0 ? 0 : d / max, v: max };
}

function hsvToRgb({ h, s, v }: Hsv): Rgb {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const sectors = [
    [c, x, 0],
    [x, c, 0],
    [0, c, x],
    [0, x, c],
    [x, 0, c],
    [c, 0, x],
  ];
  const [r, g, b] = sectors[Math.floor(h / 60) % 6];
  return { r: (r + m) * 255, g: (g + m) * 255, b: (b + m) * 255 };
}

export const hexToHsv = (hex: string) => rgbToHsv(hexToRgb(hex));
export const hsvToHex = (hsv: Hsv) => rgbToHex(hsvToRgb(hsv));

export function mixHex(a: string, b: string, t: number) {
  const x = hexToRgb(a);
  const y = hexToRgb(b);
  return rgbToHex({ r: x.r + (y.r - x.r) * t, g: x.g + (y.g - x.g) * t, b: x.b + (y.b - x.b) * t });
}

export function rgba(hex: string, opacity = 1) {
  const { r, g, b } = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${opacity})`;
}

export function paintCss(p: Paint): string {
  if (p.type === "solid") return rgba(p.color, p.opacity);
  const stops = [...p.stops]
    .sort((a, b) => a.offset - b.offset)
    .map((s) => `${rgba(s.color, s.opacity)} ${Math.round(s.offset * 1000) / 10}%`);
  // SVG rotates a left-to-right gradient by angle_deg; CSS 90deg is left-to-right.
  return `linear-gradient(${p.angle_deg + 90}deg, ${stops.join(", ")})`;
}

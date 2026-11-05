import { childrenOf } from "../../lib";
import type { Document, Layer, LayerSvgs, Paint } from "../types";

const esc = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const n = (v: number) => String(Math.round(v * 1000) / 1000);

function hash(text: string) {
  let h = 5381;
  for (let i = 0; i < text.length; i++) h = (h * 33) ^ text.charCodeAt(i);
  return (h >>> 0).toString(36);
}

function paint(p: Paint | undefined, id: string, defs: string[]) {
  if (!p) return "#D9DCE3";
  if (p.type === "solid") return p.color;
  const stops = p.stops
    .map((s) => `<stop offset="${n(s.offset)}" stop-color="${s.color}" stop-opacity="${n(s.opacity)}"/>`)
    .join("");
  defs.push(`<linearGradient id="g-${esc(id)}" x1="0" y1="0" x2="0" y2="1">${stops}</linearGradient>`);
  return `url(#g-${esc(id)})`;
}

function picture(l: Layer, fill: string) {
  const [w, h] = [l.w_mm, l.h_mm];
  const art =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${n(w)} ${n(h)}">` +
    `<ellipse cx="${n(w / 2)}" cy="${n(h / 2)}" rx="${n(w / 2)}" ry="${n(h / 2)}" fill="${fill}"/>` +
    `<text x="${n(w / 2)}" y="${n(h / 2)}" font-size="${n(Math.min(w / 8, h / 6))}" text-anchor="middle" dominant-baseline="middle" fill="rgba(0,0,0,0.45)" font-family="system-ui">${esc(l.name)}</text></svg>`;
  return `<image width="${n(w)}" height="${n(h)}" href="data:image/svg+xml,${encodeURIComponent(art)}"/>`;
}

function content(doc: Document, l: Layer, defs: string[]): string {
  const k = l.kind;
  const [w, h] = [l.w_mm, l.h_mm];
  if (k.type === "group")
    return childrenOf(doc.layers, l.id)
      .map((c) => element(doc, c, defs))
      .join("");
  const solid = l.fills[0]?.type === "solid" ? l.fills[0].color : "#D9DCE3";
  const fill = paint(l.fills[0], l.id, defs);
  const s = l.strokes[0];
  const stroke = s ? ` stroke="${paint(s.paint, `${l.id}-s`, defs)}" stroke-width="${n(s.width_mm)}"` : "";
  if (k.type === "text") {
    return `<text x="${n(w / 2)}" y="0" font-family="Pretendard, system-ui, sans-serif" font-weight="${k.font_weight}" font-size="${n(k.font_size_mm)}" text-anchor="middle" dominant-baseline="hanging" fill="${fill}"${stroke}>${esc(k.text)}</text>`;
  }
  if (l.strategy === "background") {
    return `<rect width="${n(w)}" height="${n(h)}" fill="${fill}"/><ellipse cx="${n(w / 2)}" cy="${n(h * 1.25)}" rx="${n(w * 0.62)}" ry="${n(h * 0.42)}" fill="#C9E2AC"/>`;
  }
  if (k.type === "image") return picture(l, solid);
  if (k.type === "path")
    return `<rect width="${n(w)}" height="${n(h)}" rx="${n(h / 2)}" fill="${fill}"${stroke}/>`;
  if (k.shape === "ellipse") {
    return `<ellipse cx="${n(w / 2)}" cy="${n(h / 2)}" rx="${n(w / 2)}" ry="${n(h / 2)}" fill="${fill}"${stroke}/>`;
  }
  if (k.shape === "line") {
    return `<line x1="0" y1="${n(h / 2)}" x2="${n(w)}" y2="${n(h / 2)}" stroke="${solid}" stroke-width="${n(Math.max(1, s?.width_mm ?? h / 4))}"/>`;
  }
  const rx = k.shape === "rect" ? k.rx_mm : 0;
  return `<rect width="${n(w)}" height="${n(h)}" rx="${n(rx)}" fill="${fill}"${stroke}/>`;
}

function element(doc: Document, l: Layer, defs: string[]): string {
  const rotate = l.rotation_deg ? ` rotate(${n(l.rotation_deg)} ${n(l.w_mm / 2)} ${n(l.h_mm / 2)})` : "";
  const shadow = l.effects.find((e) => e.type === "drop_shadow");
  if (shadow) {
    defs.push(
      `<filter id="f-${esc(l.id)}" x="-50%" y="-50%" width="200%" height="200%"><feDropShadow dx="${n(shadow.x_mm)}" dy="${n(shadow.y_mm)}" stdDeviation="${n(shadow.blur_mm / 2)}" flood-color="${shadow.color}" flood-opacity="${n(shadow.opacity)}"/></filter>`,
    );
  }
  const attrs = [
    `data-layer="${esc(l.id)}"`,
    `transform="translate(${n(l.x_mm)} ${n(l.y_mm)})${rotate}"`,
    l.opacity < 1 ? `opacity="${n(l.opacity)}"` : "",
    shadow ? `filter="url(#f-${esc(l.id)})"` : "",
    l.visible ? "" : `data-hidden="1" display="none"`,
  ];
  return `<g ${attrs.filter(Boolean).join(" ")}>${content(doc, l, defs)}</g>`;
}

function bbox(l: Layer): [number, number, number, number] {
  const [cx, cy] = [l.x_mm + l.w_mm / 2, l.y_mm + l.h_mm / 2];
  const [sin, cos] = [Math.sin((l.rotation_deg * Math.PI) / 180), Math.cos((l.rotation_deg * Math.PI) / 180)];
  const xs: number[] = [];
  const ys: number[] = [];
  for (const [dx, dy] of [
    [-1, -1],
    [1, -1],
    [1, 1],
    [-1, 1],
  ]) {
    const [px, py] = [(dx * l.w_mm) / 2, (dy * l.h_mm) / 2];
    xs.push(cx + px * cos - py * sin);
    ys.push(cy + px * sin + py * cos);
  }
  return [
    Math.min(...xs),
    Math.min(...ys),
    Math.max(...xs) - Math.min(...xs),
    Math.max(...ys) - Math.min(...ys),
  ];
}

export function layerSvgs(doc: Document, have: ReadonlySet<string>): LayerSvgs {
  return {
    width_mm: doc.artboard.width_mm,
    height_mm: doc.artboard.height_mm,
    assets_dir: "",
    layers: childrenOf(doc.layers, null).map((l) => {
      const defs: string[] = [];
      const body = element(doc, l, defs);
      const svg = defs.length ? body.replace(">", `><defs>${defs.join("")}</defs>`) : body;
      const key = `${l.id}-${hash(svg)}`;
      return { id: l.id, key, svg: have.has(key) ? null : svg, bbox_mm: bbox(l) };
    }),
  };
}

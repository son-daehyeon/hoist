import { childrenOf } from "../lib/tree";
import type { Document, Layer, LayerRaster, Paint } from "../types";

const MAX_SIDE = 8192;

function paintStyle(ctx: CanvasRenderingContext2D, paint: Paint | null | undefined, h: number) {
  if (!paint) return "#D9DCE3";
  if (paint.type === "solid") return paint.color;
  const g = ctx.createLinearGradient(0, 0, 0, h);
  paint.stops.forEach((s) => g.addColorStop(s.offset, s.color));
  return g;
}

function drawLeaf(ctx: CanvasRenderingContext2D, layer: Layer, s: number) {
  const k = layer.kind;
  const w = layer.w_mm * s;
  const h = layer.h_mm * s;
  if (k.type === "text") {
    ctx.font = `${k.font_weight} ${Math.max(1, k.font_size_mm * s)}px Pretendard, system-ui, sans-serif`;
    ctx.fillStyle = paintStyle(ctx, layer.fills[0], h);
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    ctx.fillText(k.text, w / 2, 0, w);
    return;
  }
  ctx.fillStyle = paintStyle(ctx, layer.fills[0], h);
  if (layer.strategy === "background") {
    ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = "#C9E2AC";
    ctx.beginPath();
    ctx.ellipse(w / 2, h * 1.25, w * 0.62, h * 0.42, 0, 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  ctx.beginPath();
  if (k.type === "image") ctx.ellipse(w / 2, h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
  else ctx.roundRect(0, 0, w, h, k.type === "shape" && k.shape === "rect" ? k.rx_mm * s : h / 2);
  ctx.fill();
  if (k.type === "image") {
    ctx.fillStyle = "rgba(0,0,0,0.45)";
    ctx.font = `500 ${Math.max(8, Math.min(w / 8, h / 6))}px system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(layer.name, w / 2, h / 2, w * 0.9);
  }
}

function drawContent(ctx: CanvasRenderingContext2D, layers: Layer[], layer: Layer, s: number) {
  ctx.globalAlpha *= layer.opacity;
  if (layer.kind.type === "group") drawChildren(ctx, layers, layer.id, s);
  else drawLeaf(ctx, layer, s);
}

function drawChildren(ctx: CanvasRenderingContext2D, layers: Layer[], parent: string | null, s: number) {
  for (const c of childrenOf(layers, parent)) {
    if (!c.visible) continue;
    ctx.save();
    ctx.translate(c.x_mm * s, c.y_mm * s);
    drawContent(ctx, layers, c, s);
    ctx.restore();
  }
}

function canvas(w: number, h: number) {
  const c = document.createElement("canvas");
  c.width = Math.max(1, Math.ceil(w));
  c.height = Math.max(1, Math.ceil(h));
  return c;
}

const stripPrefix = (url: string) => url.slice(url.indexOf(",") + 1);

export function effectiveScale(doc: Document, s: number) {
  return Math.min(s, MAX_SIDE / Math.max(doc.artboard.width_mm, doc.artboard.height_mm));
}

export function renderLayers(doc: Document, s: number): LayerRaster[] {
  const { width_mm: W, height_mm: H, background } = doc.artboard;
  const bg = canvas(W * s, H * s);
  const bctx = bg.getContext("2d");
  if (bctx) {
    bctx.fillStyle = paintStyle(bctx, background, bg.height);
    bctx.fillRect(0, 0, bg.width, bg.height);
  }
  const out: LayerRaster[] = [
    { id: "__background", png_base64: stripPrefix(bg.toDataURL("image/png")), x_px: 0, y_px: 0, w_px: bg.width, h_px: bg.height },
  ];
  for (const l of childrenOf(doc.layers, null)) {
    if (!l.visible) continue;
    const c = canvas(l.w_mm * s, l.h_mm * s);
    const ctx = c.getContext("2d");
    if (ctx) drawContent(ctx, doc.layers, l, s);
    out.push({
      id: l.id,
      png_base64: stripPrefix(c.toDataURL("image/png")),
      x_px: Math.round(l.x_mm * s),
      y_px: Math.round(l.y_mm * s),
      w_px: c.width,
      h_px: c.height,
    });
  }
  return out;
}

export function renderComposite(doc: Document, maxPx: number): string {
  const { width_mm: W, height_mm: H } = doc.artboard;
  const s = maxPx / Math.max(W, H);
  const c = canvas(W * s, H * s);
  const ctx = c.getContext("2d");
  if (!ctx) return "";
  ctx.fillStyle = paintStyle(ctx, doc.artboard.background, c.height);
  ctx.fillRect(0, 0, c.width, c.height);
  drawChildren(ctx, doc.layers, null, s);
  return c.toDataURL("image/png");
}

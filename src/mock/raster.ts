import { isEffectivelyHidden, paintOrder } from "../lib/tree";
import type { Document, Layer, LayerRaster, Paint } from "../types";

const MAX_SIDE = 8192;

function paintStyle(ctx: CanvasRenderingContext2D, paint: Paint | undefined, h: number) {
  if (!paint) return "#D9DCE3";
  if (paint.type === "solid") return paint.color;
  const g = ctx.createLinearGradient(0, 0, 0, h);
  paint.stops.forEach((s) => g.addColorStop(s.offset, s.color));
  return g;
}

function drawLayer(ctx: CanvasRenderingContext2D, layer: Layer, w: number, h: number, s: number) {
  const k = layer.kind;
  ctx.globalAlpha = layer.opacity;
  if (k.type === "text") {
    const size = Math.max(1, k.font_size_mm * s);
    ctx.font = `${k.font_weight} ${size}px Pretendard, system-ui, sans-serif`;
    ctx.fillStyle = paintStyle(ctx, layer.fills[0], h);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(k.text, w / 2, h / 2, w);
    return;
  }
  if (layer.strategy === "background") {
    ctx.fillStyle = paintStyle(ctx, layer.fills[0], h);
    ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = "#C9E2AC";
    ctx.beginPath();
    ctx.ellipse(w / 2, h * 1.25, w * 0.62, h * 0.42, 0, 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  ctx.fillStyle = paintStyle(ctx, layer.fills[0], h);
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

function canvas(w: number, h: number) {
  const c = document.createElement("canvas");
  c.width = Math.max(1, Math.min(MAX_SIDE, Math.ceil(w)));
  c.height = Math.max(1, Math.min(MAX_SIDE, Math.ceil(h)));
  return c;
}

const stripPrefix = (url: string) => url.slice(url.indexOf(",") + 1);

export function renderLayers(doc: Document, s: number): LayerRaster[] {
  return paintOrder(doc.layers)
    .filter((l) => l.kind.type !== "group")
    .map((l) => {
      const c = canvas(l.w_mm * s, l.h_mm * s);
      const ctx = c.getContext("2d");
      if (ctx) drawLayer(ctx, l, c.width, c.height, s);
      return {
        id: l.id,
        png_base64: stripPrefix(c.toDataURL("image/png")),
        x_px: Math.round(l.x_mm * s),
        y_px: Math.round(l.y_mm * s),
        w_px: c.width,
        h_px: c.height,
      };
    });
}

export function renderComposite(doc: Document, maxPx: number): string {
  const { width_mm: W, height_mm: H } = doc.artboard;
  const s = maxPx / Math.max(W, H);
  const c = canvas(W * s, H * s);
  const ctx = c.getContext("2d");
  if (!ctx) return "";
  ctx.fillStyle = "#FFFFFF";
  ctx.fillRect(0, 0, c.width, c.height);
  for (const l of paintOrder(doc.layers)) {
    if (l.kind.type === "group" || isEffectivelyHidden(doc.layers, l)) continue;
    ctx.save();
    ctx.translate(l.x_mm * s, l.y_mm * s);
    drawLayer(ctx, l, l.w_mm * s, l.h_mm * s, s);
    ctx.restore();
  }
  return stripPrefix(c.toDataURL("image/png"));
}

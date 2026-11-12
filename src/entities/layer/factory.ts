import type { Document, Layer, LayerKind, ShapeProps, Strategy } from "@/shared/api";
import { nextLayerId, RECENT_COLORS_KEY, solid } from "@/shared/lib";

export type Box = { x: number; y: number; w: number; h: number };

const SHAPE_FALLBACK = "#CFCFD4";

function layer(
  id: string,
  name: string,
  parent: string | null,
  kind: LayerKind,
  box: Box,
  strategy: Strategy,
  strategy_reason = "",
): Layer {
  return {
    id,
    name,
    parent,
    kind,
    x_mm: box.x,
    y_mm: box.y,
    w_mm: box.w,
    h_mm: box.h,
    rotation_deg: 0,
    opacity: 1,
    visible: true,
    locked: false,
    fills: [],
    strokes: [],
    effects: [],
    strategy,
    strategy_reason,
    pinned: [],
    source: null,
  };
}

function base(
  doc: Document,
  name: string,
  kind: LayerKind,
  box: Box,
  strategy: Strategy,
  color: string,
): Layer {
  const rounded = {
    x: Math.round(box.x),
    y: Math.round(box.y),
    w: Math.max(1, Math.round(box.w)),
    h: Math.max(1, Math.round(box.h)),
  };
  return { ...layer(nextLayerId(doc.layers), name, null, kind, rounded, strategy), fills: [solid(color)] };
}

export function imageLayer(
  id: string,
  name: string,
  parent: string | null,
  image: { asset: string; width_px: number; height_px: number },
  box: Box,
): Layer {
  return layer(
    id,
    name,
    parent,
    {
      type: "image",
      asset: image.asset,
      natural_w_px: image.width_px,
      natural_h_px: image.height_px,
      fit: "contain",
      crop_px: null,
      adjust: { brightness: 0, contrast: 0, saturation: 0 },
    },
    box,
    "user_image",
    "사용자가 직접 넣은 그림",
  );
}

export function textLayer(doc: Document, cx: number, cy: number): Layer {
  const size = Math.round(doc.artboard.height_mm * 0.12);
  const w = size * 4;
  const h = size * 1.4;
  return base(
    doc,
    "글자",
    {
      type: "text",
      text: "글자",
      font_family: "Pretendard",
      font_weight: 700,
      font_size_mm: size,
      letter_spacing_mm: 0,
      line_height: 1.2,
      align: "center",
      scale_x: 1,
      skew_deg: 0,
      vertical: false,
      arc_bend: 0,
    },
    { x: cx - w / 2, y: cy - h / 2, w, h },
    "text",
    "#18181B",
  );
}

export function rectLayer(doc: Document, box: Box): Layer {
  return base(
    doc,
    "사각형",
    { type: "shape", shape: "rect", rx_mm: 0 },
    box,
    "native_vector",
    SHAPE_FALLBACK,
  );
}

export function defaultRect(doc: Document, cx: number, cy: number): Box {
  const w = doc.artboard.width_mm * 0.1;
  const h = doc.artboard.height_mm * 0.2;
  return { x: cx - w / 2, y: cy - h / 2, w, h };
}

export function lastColor(): string {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(RECENT_COLORS_KEY) ?? "[]");
    return Array.isArray(v) && typeof v[0] === "string" ? v[0] : SHAPE_FALLBACK;
  } catch {
    return SHAPE_FALLBACK;
  }
}

export const lineWidth = (doc: Document) =>
  Math.max(1, Math.round(Math.min(doc.artboard.width_mm, doc.artboard.height_mm) * 0.01));

export const shapeSize = (doc: Document) =>
  Math.round(Math.min(doc.artboard.width_mm, doc.artboard.height_mm) * 0.2);

export function shapeLayer(
  doc: Document,
  name: string,
  shape: ShapeProps,
  frame: Box & { deg: number },
  color: string,
): Layer {
  const line = shape.shape === "line";
  return {
    ...base(doc, name, { type: "shape", ...shape }, frame, "native_vector", color),
    rotation_deg: Math.round(frame.deg * 10) / 10,
    fills: line ? [] : [solid(color)],
    strokes: line
      ? [
          {
            paint: solid(color),
            width_mm: lineWidth(doc),
            align: "center",
            join: "miter",
            dash_mm: [],
          },
        ]
      : [],
  };
}

export function pathLayer(doc: Document, d: string, box: Box, closed: boolean, color: string): Layer {
  const exact = (v: number) => Math.round(v * 100) / 100;
  return {
    ...base(doc, "패스", { type: "path", d, fill_rule: "nonzero" }, box, "native_vector", color),
    x_mm: exact(box.x),
    y_mm: exact(box.y),
    w_mm: exact(box.w),
    h_mm: exact(box.h),
    fills: closed ? [solid(color)] : [],
    strokes: closed
      ? []
      : [
          {
            paint: solid(color),
            width_mm: lineWidth(doc),
            align: "center",
            join: "round",
            dash_mm: [],
          },
        ],
  };
}

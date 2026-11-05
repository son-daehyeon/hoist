import type { Document, Layer, LayerKind, ShapeProps, Strategy } from "@/shared/api";
import { nextLayerId } from "@/shared/lib";

export type Box = { x: number; y: number; w: number; h: number };

function base(
  doc: Document,
  name: string,
  kind: LayerKind,
  box: Box,
  strategy: Strategy,
  color: string,
): Layer {
  return {
    id: nextLayerId(doc.layers),
    name,
    parent: null,
    kind,
    x_mm: Math.round(box.x),
    y_mm: Math.round(box.y),
    w_mm: Math.max(1, Math.round(box.w)),
    h_mm: Math.max(1, Math.round(box.h)),
    rotation_deg: 0,
    opacity: 1,
    visible: true,
    locked: false,
    fills: [{ type: "solid", color, opacity: 1 }],
    strokes: [],
    effects: [],
    strategy,
    strategy_reason: "",
    pinned: [],
    source: null,
  };
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
  return base(doc, "사각형", { type: "shape", shape: "rect", rx_mm: 0 }, box, "native_vector", "#CFCFD4");
}

export function defaultRect(doc: Document, cx: number, cy: number): Box {
  const w = doc.artboard.width_mm * 0.1;
  const h = doc.artboard.height_mm * 0.2;
  return { x: cx - w / 2, y: cy - h / 2, w, h };
}

const RECENT_COLORS = "hoist.recent-colors";
const SHAPE_FALLBACK = "#CFCFD4";

export function lastColor(): string {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(RECENT_COLORS) ?? "[]");
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
    fills: line ? [] : [{ type: "solid", color, opacity: 1 }],
    strokes: line
      ? [
          {
            paint: { type: "solid", color, opacity: 1 },
            width_mm: lineWidth(doc),
            align: "center",
            join: "miter",
            dash_mm: [],
          },
        ]
      : [],
  };
}

import type { Document, Layer, LayerKind, Paint, Strategy } from "../types";

const solid = (color: string): Paint => ({ type: "solid", color, opacity: 1 });

function text(content: string, size: number, weight = 700): LayerKind {
  return {
    type: "text",
    text: content,
    font_family: "Pretendard",
    font_weight: weight,
    font_size_mm: size,
    letter_spacing_mm: 0,
    line_height: 1.2,
    align: "center",
  };
}

export function makeLayer(
  id: string,
  name: string,
  parent: string | null,
  kind: LayerKind,
  box: [number, number, number, number],
  strategy: Strategy,
  fills: Paint[] = [],
): Layer {
  const [x_mm, y_mm, w_mm, h_mm] = box;
  return {
    id,
    name,
    parent,
    kind,
    x_mm,
    y_mm,
    w_mm,
    h_mm,
    rotation_deg: 0,
    opacity: 1,
    visible: true,
    locked: false,
    fills,
    strokes: [],
    effects: [],
    strategy,
    strategy_reason: "",
    pinned: [],
    source: null,
  };
}

export function sampleLayers(W: number, H: number, titles: [string, string, string]): Layer[] {
  const b = (x: number, y: number, w: number, h: number): [number, number, number, number] => [
    Math.round(x * W),
    Math.round(y * H),
    Math.round(w * W),
    Math.round(h * H),
  ];
  const group: LayerKind = { type: "group" };
  const img = (asset: string): LayerKind => ({
    type: "image",
    asset,
    natural_w_px: 512,
    natural_h_px: 512,
    fit: "contain",
  });
  const [title, sub, badge] = titles;
  return [
    makeLayer("L1", "배경", null, group, b(0, 0, 1, 1), "background"),
    makeLayer("L2", "하늘 · 들판", "L1", img("assets/bg.png"), b(0, 0, 1, 1), "background", [
      {
        type: "linear_gradient",
        angle_deg: 180,
        stops: [
          { offset: 0, color: "#DCEBFA", opacity: 1 },
          { offset: 1, color: "#F6F9FC", opacity: 1 },
        ],
      },
    ]),
    makeLayer("L3", "그래픽", null, group, b(0, 0, 1, 1), "native_vector"),
    makeLayer("L4", "문인화", "L3", img("assets/art.png"), b(0.84, 0.12, 0.11, 0.6), "generated_raster", [solid("#E6DAF2")]),
    makeLayer("L5", "어르신 그림", "L3", img("assets/elder.png"), b(0.05, 0.12, 0.11, 0.6), "generated_raster", [solid("#F7CDA6")]),
    makeLayer("L6", "정보 띠", "L3", { type: "shape", shape: "rect", rx_mm: 20 }, b(0.22, 0.8, 0.56, 0.1), "native_vector", [solid("#FFFFFF")]),
    makeLayer("L7", "파란 붓터치 띠", "L3", { type: "path", d: "M0 0H1V1H0Z", fill_rule: "nonzero" }, b(0.425, 0.06, 0.15, 0.13), "generated_raster", [solid("#2F5DA8")]),
    makeLayer("L8", "글자", null, group, b(0, 0, 1, 1), "text_traced"),
    makeLayer("L9", badge, "L8", text(badge, H * 0.07, 600), b(0.43, 0.07, 0.14, 0.11), "text_traced", [solid("#FFFFFF")]),
    makeLayer("L10", sub, "L8", text(sub, H * 0.08, 600), b(0.37, 0.56, 0.26, 0.12), "text_traced", [solid("#5B3F9E")]),
    makeLayer("L11", title, "L8", text(title, H * 0.2, 700), b(0.36, 0.22, 0.28, 0.27), "text_traced", [solid("#1E3F7A")]),
  ];
}

export function emptyDocument(W: number, H: number): Document {
  return { rev: 0, artboard: { width_mm: W, height_mm: H, bleed_mm: 0, background: solid("#FFFFFF") }, layers: [] };
}

export const SAMPLES: { name: string; w: number; h: number; titles: [string, string, string]; daysAgo: number }[] = [
  { name: "청전동 주민역량강화", w: 6000, h: 900, titles: ["청전동 주민역량강화", "참여자 모집", "2026년 하반기"], daysAgo: 0 },
  { name: "대지농산 햇고구마", w: 4000, h: 700, titles: ["대지농산 햇고구마", "산지 직송", "가을 수확"], daysAgo: 7 },
  { name: "경로당 가을 잔치", w: 5000, h: 900, titles: ["경로당 가을 잔치", "어르신 모두 오세요", "10월 25일"], daysAgo: 16 },
];

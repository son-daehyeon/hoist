import type { Layer, Strategy } from "../types";
import type { IconName } from "../ui";

export function pngSrc(s: string): string;
export function pngSrc(s: string | null | undefined): string | undefined;
export function pngSrc(s: string | null | undefined) {
  if (!s) return undefined;
  return s.startsWith("data:") || s.startsWith("blob:") ? s : `data:image/png;base64,${s}`;
}

export const mmToPx = (mm: number, dpi: number) => Math.round((mm / 25.4) * dpi);

export const sizeLabel = (w: number, h: number) => `${Math.round(w)} × ${Math.round(h)} mm`;

export const dpiLabel = (w: number, h: number, dpi: number) =>
  `${dpi}dpi 기준 ${mmToPx(w, dpi)} × ${mmToPx(h, dpi)} px`;

export function ratioLabel(w: number, h: number) {
  if (!w || !h) return "–";
  const r = w >= h ? w / h : h / w;
  const s = String(Number(r.toFixed(1)));
  return w >= h ? `${s} : 1` : `1 : ${s}`;
}

const pad = (n: number) => String(n).padStart(2, "0");
const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

function dayLabel(d: Date) {
  const now = new Date();
  const md = `${d.getMonth() + 1}월 ${d.getDate()}일`;
  return d.getFullYear() === now.getFullYear() ? md : `${d.getFullYear()}년 ${md}`;
}

export function dateLabel(iso: string) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return sameDay(d, new Date()) ? `오늘 ${pad(d.getHours())}:${pad(d.getMinutes())}` : dayLabel(d);
}

export function timeLabel(iso: string) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return sameDay(d, new Date()) ? hm : `${dayLabel(d)} ${hm}`;
}

export const STRATEGY_SHORT: Record<Strategy, string> = {
  native_vector: "도형",
  text_traced: "트레이싱",
  text: "글자",
  traced_vector: "벡터",
  generated_raster: "생성",
  background: "배경",
  original_crop: "원본",
};

export const STRATEGY_LONG: Record<Strategy, string> = {
  native_vector: "도형",
  text_traced: "트레이싱 글자",
  text: "편집 글자",
  traced_vector: "트레이싱 벡터",
  generated_raster: "생성 그림",
  background: "배경",
  original_crop: "원본 조각",
};

export function layerIcon(layer: Layer): IconName {
  switch (layer.kind.type) {
    case "group":
      return "folder";
    case "text":
      return "type";
    case "image":
      return "image";
    default:
      return "shape";
  }
}

export function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

export function errorMessage(e: unknown) {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "문제가 생겼어요";
}

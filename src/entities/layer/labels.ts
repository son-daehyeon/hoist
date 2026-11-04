import type { Layer, Strategy } from "@/shared/api";
import type { IconName } from "@/shared/ui";

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

import type { ShapeTool } from "@/entities/editor";
import type { ShapeProps } from "@/shared/api";
import type { Params } from "./drag";

type Kind = ShapeTool["kind"];

export const STAR_INNER = 0.5;

export const SHAPE_LABEL: Record<Kind, string> = {
  rect: "사각형",
  rounded: "둥근 사각형",
  ellipse: "원",
  line: "선",
  polygon: "다각형",
  star: "별",
};

export const ROUND_RATIO = 0.2;
const radius = (f: Params) => Math.round(Math.min(f.w, f.h) * ROUND_RATIO * 10) / 10;

export function shapeProps(t: ShapeTool, f: Params): ShapeProps {
  switch (t.kind) {
    case "rect":
      return { shape: "rect", rx_mm: 0 };
    case "rounded":
      return { shape: "rect", rx_mm: radius(f) };
    case "ellipse":
      return { shape: "ellipse" };
    case "line":
      return { shape: "line", arrow_start: false, arrow_end: t.arrow };
    case "polygon":
      return { shape: "polygon", sides: t.sides };
    case "star":
      return { shape: "star", points: t.points, inner_ratio: STAR_INNER };
  }
}

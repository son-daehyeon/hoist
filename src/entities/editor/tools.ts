import { useEffect, useState } from "react";

const OUTLINE_KEY = "hoist.outline";
const SHAPE_KEY = "hoist.shape-tool";

export type Tool = "select" | "hand" | "text" | "shape" | "pen" | "capture";
export const SHAPE_KINDS = ["rect", "rounded", "ellipse", "line", "polygon", "star"] as const;
export type ShapeTool = { kind: (typeof SHAPE_KINDS)[number]; arrow: boolean; sides: number; points: number };
const SHAPE_DEFAULT: ShapeTool = { kind: "rect", arrow: false, sides: 6, points: 5 };

function loadShapeTool(): ShapeTool {
  try {
    const v = {
      ...SHAPE_DEFAULT,
      ...(JSON.parse(localStorage.getItem(SHAPE_KEY) ?? "{}") as Partial<ShapeTool>),
    };
    const count = (n: unknown) => typeof n === "number" && n >= 3 && n <= 64;
    return SHAPE_KINDS.includes(v.kind) && typeof v.arrow === "boolean" && count(v.sides) && count(v.points)
      ? v
      : SHAPE_DEFAULT;
  } catch {
    return SHAPE_DEFAULT;
  }
}

export function useToolPrefs() {
  const [outline, setOutline] = useState(() => localStorage.getItem(OUTLINE_KEY) === "1");
  const [shapeTool, setShapeTool] = useState(loadShapeTool);

  useEffect(() => localStorage.setItem(OUTLINE_KEY, outline ? "1" : "0"), [outline]);
  useEffect(() => localStorage.setItem(SHAPE_KEY, JSON.stringify(shapeTool)), [shapeTool]);

  return { outline, setOutline, shapeTool, setShapeTool };
}

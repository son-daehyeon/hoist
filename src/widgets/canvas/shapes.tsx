import { useState } from "react";
import { SHAPE_KINDS, useEd, type ShapeTool } from "@/entities/editor";
import { closedPath, polygonPoints, starPoints } from "@/shared/lib";
import { Checkbox, Popover, ScrubField, ToolButton } from "@/shared/ui";
import type { Params } from "./drag";
import { ROUND_RATIO, SHAPE_LABEL, STAR_INNER } from "./shapeProps";

function Outline({ t, w, h }: { t: ShapeTool; w: number; h: number }) {
  switch (t.kind) {
    case "rect":
    case "rounded":
      return <rect width={w} height={h} rx={t.kind === "rounded" ? Math.min(w, h) * ROUND_RATIO : 0} />;
    case "ellipse":
      return <ellipse cx={w / 2} cy={h / 2} rx={w / 2} ry={h / 2} />;
    case "line":
      return <line x1={0} y1={h / 2} x2={w} y2={h / 2} />;
    case "polygon":
      return <path d={closedPath(polygonPoints(w, h, t.sides))} />;
    case "star":
      return <path d={closedPath(starPoints(w, h, t.points, STAR_INNER))} />;
  }
}

function ShapeGlyph({ t }: { t: ShapeTool }) {
  const line = t.kind === "line";
  return (
    <svg viewBox="0 0 24 24" width={20} height={20} className="cv-glyph" aria-hidden>
      {line ? (
        <>
          <path d="M4 20L20 4" />
          {t.arrow && <path d="M12 4h8v8" />}
        </>
      ) : (
        <g transform="translate(4 4)">
          <Outline t={t} w={16} h={16} />
        </g>
      )}
    </svg>
  );
}

export function ShapeDraft({ t, f }: { t: ShapeTool; f: Params }) {
  return (
    <g className="cv-draft" transform={`translate(${f.x} ${f.y}) rotate(${f.deg} ${f.w / 2} ${f.h / 2})`}>
      <Outline t={t} w={f.w} h={f.h} />
    </g>
  );
}

export function ShapeToolButton() {
  const { tool, setTool, shapeTool, setShapeTool } = useEd();
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const set = (patch: Partial<ShapeTool>) => {
    setShapeTool((t) => ({ ...t, ...patch }));
    setTool("shape");
  };
  const count =
    shapeTool.kind === "polygon"
      ? { label: "변", value: shapeTool.sides, key: "sides" as const }
      : shapeTool.kind === "star"
        ? { label: "꼭짓점", value: shapeTool.points, key: "points" as const }
        : null;

  return (
    <>
      <ToolButton
        icon="square"
        label="도형"
        shortcut="R"
        active={tool === "shape"}
        onClick={(e) => {
          setTool("shape");
          setAnchor(anchor ? null : e.currentTarget);
        }}
      />
      {anchor && (
        <Popover anchor={anchor} side="right" width={232} label="도형 고르기" onClose={() => setAnchor(null)}>
          <div className="cv-shapes" role="radiogroup" aria-label="도형">
            {SHAPE_KINDS.map((k) => (
              <button
                key={k}
                type="button"
                role="radio"
                aria-checked={shapeTool.kind === k}
                className={`cv-shape-opt ${shapeTool.kind === k ? "is-active" : ""}`}
                onClick={() => set({ kind: k })}
              >
                <ShapeGlyph t={{ ...shapeTool, kind: k }} />
                <span className="t-micro">{SHAPE_LABEL[k]}</span>
              </button>
            ))}
          </div>
          {shapeTool.kind === "line" && (
            <div className="cv-shape-more">
              <Checkbox checked={shapeTool.arrow} onChange={(arrow) => set({ arrow })}>
                끝에 화살표
              </Checkbox>
            </div>
          )}
          {count && (
            <div className="cv-shape-more">
              <ScrubField
                label={count.label}
                value={count.value}
                min={3}
                max={20}
                step={1}
                decimals={0}
                onCommit={(v) => set({ [count.key]: v })}
              />
            </div>
          )}
        </Popover>
      )}
    </>
  );
}

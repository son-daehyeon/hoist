import { useRef, type CSSProperties, type RefObject } from "react";
import { useEd } from "@/entities/editor";
import { ObjectContextMenu } from "@/features/object-actions";
import type { Box } from "@/entities/layer";
import type { Layer } from "@/shared/api";
import { isEffectivelyHidden, sizeLabel } from "@/shared/lib";
import { TooltipBubble } from "@/shared/ui";
import { useLayerFragments, useOriginalSrc } from "./canvasData";
import { centerOf, inkFrame, type Frame } from "./geometry";
import { useCanvasPointer } from "./pointer";
import { useLivePreview } from "./preview";
import { canTransform, frameFor, frameOf } from "./selection";
import { AgentCursors } from "./AgentCursors";
import { CropView } from "./CropView";
import { PathView } from "./PathView";
import { ShapeDraft } from "./shapes";
import { TextEditor } from "./TextEditor";
import { TransformHandles } from "./TransformHandles";
import { SvgLayer } from "./SvgLayer";
import { LABEL_H, paneRect, type Panes, type Rect, type View } from "./viewport";

const SAFE_MM = 50;

const px = (r: Rect): CSSProperties => ({ left: r.x, top: r.y, width: r.w, height: r.h });
const onScreen = (pane: Rect, b: Box, z: number): Rect => ({
  x: pane.x + b.x * z,
  y: pane.y + b.y * z,
  w: b.w * z,
  h: b.h * z,
});
const turned = (pane: Rect, p: Frame, z: number): CSSProperties => ({
  ...px(onScreen(pane, p, z)),
  transform: p.deg ? `rotate(${p.deg}deg)` : undefined,
});

function ImageBox({ layer, p }: { layer: Layer; p: Frame }) {
  const c = centerOf(p);
  const { x, y, w, h } = p;
  return (
    <g
      data-layer={layer.id}
      className="cv-imgbox"
      transform={p.deg ? `rotate(${p.deg} ${c.x} ${c.y})` : undefined}
    >
      <rect x={x} y={y} width={w} height={h} />
      <path d={`M${x} ${y}L${x + w} ${y + h}M${x + w} ${y}L${x} ${y + h}`} />
    </g>
  );
}

export function CanvasView({
  viewportRef,
  view,
  setView,
  panes,
}: {
  viewportRef: RefObject<HTMLDivElement | null>;
  view: View;
  setView: (fn: (v: View) => View) => void;
  panes: Panes;
}) {
  const ed = useEd();
  const { projectId, project, doc, selection, compare, guides, captures, busy, outline, shapeTool } = ed;
  const { editing } = ed;
  const svgRef = useRef<SVGSVGElement>(null);
  const worldRef = useRef<SVGGElement>(null);
  const pointer = useCanvasPointer(viewportRef, svgRef, view, setView, panes);
  const { drag, live, clearPreview } = pointer;

  const W = doc?.artboard.width_mm ?? 1;
  const H = doc?.artboard.height_mm ?? 1;
  const z = view.zoom;
  const rev = doc?.rev;
  const original = project?.originals.at(-1);
  const originalVersion = original?.version;

  const { fragments, inks } = useLayerFragments(projectId, rev, clearPreview);
  useLivePreview(svgRef, worldRef, live);
  const originalSrc = useOriginalSrc(projectId, originalVersion);

  if (!doc) return <div ref={viewportRef} className="cv-viewport" />;

  const layers = doc.layers;
  const work = panes.work;
  const bg = doc.artboard.background;
  const artboardPx = (b: Box) => px({ x: b.x * z, y: b.y * z, w: b.w * z, h: b.h * z });
  const shown = (l: Layer) => live?.next[l.id] ?? frameOf(layers, l);
  const inked = (l: Layer) => (editing ? shown(l) : inkFrame(shown(l), inks[l.id]));
  const selected = selection.map((id) => layers.find((l) => l.id === id)).filter((l) => l !== undefined);
  const frame = selected.length === 1 ? inked(selected[0]) : (pointer.frame ?? frameFor(selected.map(shown)));
  const edited = editing && layers.find((l) => l.id === editing.id);
  const editable = frame && !editing && canTransform(layers, selection);
  const images = outline
    ? layers.filter((l) => l.kind.type === "image" && !isEffectivelyHidden(layers, l))
    : [];

  return (
    <>
      <div ref={viewportRef} className="cv-viewport" style={{ cursor: pointer.cursor }} {...pointer.handlers}>
        {panes.original && (
          <>
            <div
              className="cv-label t-micro"
              style={{ left: panes.original.x, top: panes.original.y - LABEL_H + 4 }}
            >
              원본
            </div>
            <div className="cv-artboard" style={px(panes.original)}>
              {originalSrc && <img className="cv-fill" src={originalSrc} alt="" draggable={false} />}
            </div>
          </>
        )}

        <div
          className={`cv-label t-micro ${selection.length ? "is-active" : ""}`}
          style={{ left: work.x, top: work.y - LABEL_H + 4 }}
        >
          작업본 · {busy ? "만드는 중" : sizeLabel(W, H)}
        </div>
        <div
          className="cv-artboard"
          style={{ ...px(work), background: !outline && bg?.type === "solid" ? bg.color : "#FFFFFF" }}
        >
          <svg
            ref={svgRef}
            className={`cv-svg ${outline ? "is-outline" : ""} ${drag?.kind === "edit" ? "is-dragging" : ""}`}
            viewBox={`0 0 ${W} ${H}`}
            width={work.w}
            height={work.h}
            preserveAspectRatio="none"
          >
            <g ref={worldRef}>
              {fragments.map((f) => (
                <SvgLayer key={`${f.id}:${f.key}`} svg={f.svg} />
              ))}
              {images.map((l) => (
                <ImageBox key={l.id} layer={l} p={frameOf(layers, l)} />
              ))}
              {drag?.kind === "draw" && drag.dragged && <ShapeDraft t={shapeTool} f={drag.frame} />}
            </g>
          </svg>
          {compare > 0 && originalSrc && (
            <img
              className="cv-fill"
              src={originalSrc}
              alt=""
              style={{ opacity: compare }}
              draggable={false}
            />
          )}
          {guides && (
            <>
              {doc.artboard.bleed_mm > 0 && (
                <div
                  className="cv-guide is-bleed"
                  style={artboardPx({
                    x: doc.artboard.bleed_mm,
                    y: doc.artboard.bleed_mm,
                    w: W - doc.artboard.bleed_mm * 2,
                    h: H - doc.artboard.bleed_mm * 2,
                  })}
                />
              )}
              <div
                className="cv-guide"
                style={artboardPx({ x: SAFE_MM, y: SAFE_MM, w: W - SAFE_MM * 2, h: H - SAFE_MM * 2 })}
              />
            </>
          )}
        </div>

        {captures.map((c, i) => (
          <div
            key={i}
            className="cv-capture"
            style={px(onScreen(paneRect(panes, c.pane), { x: c.x_mm, y: c.y_mm, w: c.w_mm, h: c.h_mm }, z))}
          />
        ))}
        {drag?.kind === "rect" && (
          <div className="cv-capture" style={px(onScreen(paneRect(panes, drag.pane), drag.box, z))} />
        )}

        {drag?.kind === "edit" &&
          drag.guides.map((g) => (
            <div
              key={`${g.axis}${g.at}`}
              className="cv-snap"
              style={
                g.axis === "x"
                  ? { left: work.x + g.at * z, top: work.y, width: 1, height: H * z }
                  : { left: work.x, top: work.y + g.at * z, width: W * z, height: 1 }
              }
            />
          ))}
        {selected.map((l) => (
          <div key={l.id} className="cv-sel" style={turned(work, inked(l), z)} />
        ))}
        {frame && selected.length > 1 && <div className="cv-sel is-union" style={turned(work, frame, z)} />}
        {editable && <TransformHandles frame={frame} pane={work} z={z} />}
        {edited && (
          <>
            <style>{`.cv-svg [data-layer="${CSS.escape(edited.id)}"] { visibility: hidden; }`}</style>
            {editing.kind === "crop" && (
              <CropView
                layer={edited}
                p={frameOf(layers, edited)}
                edges={editing.edges}
                href={editing.href}
                z={z}
                pane={work}
              />
            )}
            {editing.kind === "text" && (
              <TextEditor
                key={edited.id}
                layer={edited}
                p={frameOf(layers, edited)}
                z={z}
                pane={work}
                outline={outline}
                pending={!!editing.pending}
                onClose={(text) => pointer.finishText(edited.id, text)}
              />
            )}
          </>
        )}
        {pointer.pathView && <PathView {...pointer.pathView} z={z} pane={work} />}
        <AgentCursors svgRef={svgRef} worldRef={worldRef} pane={work} z={z} />
        {pointer.bubble && (
          <TooltipBubble
            label="캡처를 채팅에 붙였어요"
            style={{
              position: "absolute",
              left: pointer.bubble.x,
              top: pointer.bubble.y,
              transform: "translateX(-100%)",
            }}
          />
        )}
      </div>
      {pointer.menu && <ObjectContextMenu {...pointer.menu} onClose={pointer.closeMenu} />}
    </>
  );
}

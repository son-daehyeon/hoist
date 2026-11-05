import { useEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import { useEd } from "@/entities/editor";
import { ObjectContextMenu } from "@/features/object-actions";
import type { Box } from "@/entities/layer";
import { api, errorMessage, type Layer } from "@/shared/api";
import { isEffectivelyHidden, pngSrc, sizeLabel } from "@/shared/lib";
import { toast, TooltipBubble } from "@/shared/ui";
import { aabb, centerOf, HANDLE_UV, pointOn, type Params } from "./drag";
import { FitModePicker } from "./FitModePicker";
import { HANDLES, useCanvasPointer } from "./pointer";
import { useLivePreview } from "./preview";
import { canTransform, frameFor, paramsOf } from "./selection";
import { rememberAssetsDir } from "./commands";
import { CropView } from "./CropView";
import { PathView } from "./PathView";
import { ShapeDraft } from "./shapes";
import { TextEditor } from "./TextEditor";
import { SvgLayer } from "./SvgLayer";
import { LABEL_H, type Panes, type Rect, type View } from "./viewport";

const SAFE_MM = 50;
const KNOB_PX = 22;

type Fragment = { id: string; key: string; svg: string };

const px = (r: Rect): CSSProperties => ({ left: r.x, top: r.y, width: r.w, height: r.h });
const onScreen = (pane: Rect, b: Box, z: number): Rect => ({
  x: pane.x + b.x * z,
  y: pane.y + b.y * z,
  w: b.w * z,
  h: b.h * z,
});
const turned = (pane: Rect, p: Params, z: number): CSSProperties => ({
  ...px(onScreen(pane, p, z)),
  transform: p.deg ? `rotate(${p.deg}deg)` : undefined,
});

function ImageBox({ layer, p }: { layer: Layer; p: Params }) {
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
  const svgRef = useRef<SVGSVGElement>(null);
  const worldRef = useRef<SVGGElement>(null);
  const known = useRef(new Map<string, string>());
  const [fragments, setFragments] = useState<Fragment[]>([]);
  const [originalSrc, setOriginalSrc] = useState<string>();
  const pointer = useCanvasPointer(viewportRef, svgRef, view, setView, panes);
  const { drag, live, clearPreview } = pointer;

  const W = doc?.artboard.width_mm ?? 1;
  const H = doc?.artboard.height_mm ?? 1;
  const z = view.zoom;
  const rev = doc?.rev;
  const original = project?.originals.at(-1);
  const originalVersion = original?.version;

  useEffect(() => {
    if (rev === undefined) return;
    let alive = true;
    const t = window.setTimeout(() => {
      api
        .layerSvgs(projectId, [...known.current.keys()])
        .then((r) => {
          if (!alive) return;
          const byKey = new Map<string, string>();
          const list: Fragment[] = [];
          for (const l of r.layers) {
            const svg = l.svg ?? known.current.get(l.key);
            if (svg === undefined) continue;
            byKey.set(l.key, svg);
            list.push({ id: l.id, key: l.key, svg });
          }
          known.current = byKey;
          rememberAssetsDir(r.assets_dir);
          setFragments(list);
          clearPreview();
        })
        .catch((e) => alive && toast(errorMessage(e), "error"));
    }, 16);
    return () => {
      alive = false;
      window.clearTimeout(t);
    };
  }, [projectId, rev, clearPreview]);

  useLivePreview(svgRef, worldRef, live);

  if (originalVersion === undefined && originalSrc !== undefined) setOriginalSrc(undefined);

  useEffect(() => {
    if (originalVersion === undefined) return;
    let alive = true;
    api
      .renderOriginal(projectId, originalVersion, 2400)
      .then((s) => alive && setOriginalSrc(pngSrc(s)))
      .catch((e) => alive && toast(errorMessage(e), "error"));
    return () => {
      alive = false;
    };
  }, [projectId, originalVersion]);

  if (!doc) return <div ref={viewportRef} className="cv-viewport" />;

  const layers = doc.layers;
  const work = panes.work;
  const bg = doc.artboard.background;
  const mm = (b: Box) => px({ x: b.x * z, y: b.y * z, w: b.w * z, h: b.h * z });
  const shown = (l: Layer) => live?.next[l.id] ?? paramsOf(layers, l);
  const selected = selection.map((id) => layers.find((l) => l.id === id)).filter((l) => l !== undefined);
  const frame = pointer.frame ?? frameFor(selected.map(shown));
  const editing = pointer.editing;
  const edited = editing && layers.find((l) => l.id === editing.id);
  const editable = frame && !editing && canTransform(layers, selection);
  const images = outline
    ? layers.filter((l) => l.kind.type === "image" && !isEffectivelyHidden(layers, l))
    : [];

  const screenAt = (u: number, v: number) => {
    const q = pointOn(frame!, u, v);
    return { x: work.x + q.x * z, y: work.y + q.y * z };
  };

  return (
    <div ref={viewportRef} className="cv-viewport" style={{ cursor: pointer.cursor }} {...pointer.handlers}>
      {panes.original && (
        <>
          <div
            className="cv-label t-micro"
            style={{ left: panes.original.x, top: panes.original.y - LABEL_H + 4 }}
          >
            원본 · v{original?.version} · {original?.width_px} × {original?.height_px} px
            <FitModePicker />
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
              <ImageBox key={l.id} layer={l} p={paramsOf(layers, l)} />
            ))}
            {drag?.kind === "draw" && drag.dragged && <ShapeDraft t={shapeTool} f={drag.frame} />}
          </g>
        </svg>
        {compare > 0 && originalSrc && (
          <img className="cv-fill" src={originalSrc} alt="" style={{ opacity: compare }} draggable={false} />
        )}
        {guides && (
          <>
            {doc.artboard.bleed_mm > 0 && (
              <div
                className="cv-guide is-bleed"
                style={mm({
                  x: doc.artboard.bleed_mm,
                  y: doc.artboard.bleed_mm,
                  w: W - doc.artboard.bleed_mm * 2,
                  h: H - doc.artboard.bleed_mm * 2,
                })}
              />
            )}
            <div
              className="cv-guide"
              style={mm({ x: SAFE_MM, y: SAFE_MM, w: W - SAFE_MM * 2, h: H - SAFE_MM * 2 })}
            />
          </>
        )}
      </div>

      {captures.map((c, i) => (
        <div
          key={i}
          className="cv-capture"
          style={px(
            onScreen(
              c.pane === "work" ? work : (panes.original ?? work),
              { x: c.x_mm, y: c.y_mm, w: c.w_mm, h: c.h_mm },
              z,
            ),
          )}
        />
      ))}
      {drag?.kind === "rect" && (
        <div
          className="cv-capture"
          style={px(onScreen(drag.pane === "work" ? work : (panes.original ?? work), drag.box, z))}
        />
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
        <div key={l.id} className="cv-sel" style={turned(work, shown(l), z)} />
      ))}
      {frame && selected.length > 1 && <div className="cv-sel is-union" style={turned(work, frame, z)} />}
      {editable &&
        (() => {
          const top = screenAt(0.5, 0);
          const rad = (frame.deg * Math.PI) / 180;
          const knob = { x: top.x + Math.sin(rad) * KNOB_PX, y: top.y - Math.cos(rad) * KNOB_PX };
          const box = aabb(frame);
          return (
            <>
              <div
                className="cv-knob-line"
                style={{
                  left: top.x,
                  top: top.y - KNOB_PX,
                  height: KNOB_PX,
                  transform: `rotate(${frame.deg}deg)`,
                }}
              />
              <div data-handle="rotate" className="cv-knob" style={{ left: knob.x - 5, top: knob.y - 5 }} />
              {HANDLES.map((h) => {
                const s = screenAt(...HANDLE_UV[h]);
                return (
                  <div
                    key={h}
                    data-handle={h}
                    className={`cv-handle h-${h}`}
                    style={{ left: s.x - 4, top: s.y - 4 }}
                  />
                );
              })}
              <div
                className="cv-size t-micro"
                style={{ left: work.x + (box.x + box.w / 2) * z, top: work.y + (box.y + box.h) * z + 6 }}
              >
                {Math.round(frame.w)} × {Math.round(frame.h)} mm
                {Math.round(frame.deg) % 360 !== 0 ? ` · ${Math.round(frame.deg)}°` : ""}
              </div>
            </>
          );
        })()}
      {edited && (
        <>
          <style>{`.cv-svg [data-layer="${CSS.escape(edited.id)}"] { visibility: hidden; }`}</style>
          {editing.kind === "crop" && (
            <CropView
              layer={edited}
              p={paramsOf(layers, edited)}
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
              p={paramsOf(layers, edited)}
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
      {pointer.menu && <ObjectContextMenu {...pointer.menu} onClose={pointer.closeMenu} />}
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
  );
}

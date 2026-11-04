import { useEffect, useMemo, useState, type CSSProperties, type RefObject } from "react";
import { useEd } from "@/entities/editor";
import type { Box } from "@/entities/layer";
import { api, errorMessage, type RenderResult } from "@/shared/api";
import { isEffectivelyHidden, pngSrc, sizeLabel } from "@/shared/lib";
import { toast, TooltipBubble } from "@/shared/ui";
import { boxOf, HANDLES, useCanvasPointer, type Handle } from "./pointer";
import { LABEL_H, renderScale, type Panes, type Rect, type View } from "./viewport";

const SAFE_MM = 50;

const px = (r: Rect): CSSProperties => ({ left: r.x, top: r.y, width: r.w, height: r.h });
const onScreen = (pane: Rect, b: Box, z: number): Rect => ({
  x: pane.x + b.x * z,
  y: pane.y + b.y * z,
  w: b.w * z,
  h: b.h * z,
});

function handleStyle(r: Rect, h: Handle): CSSProperties {
  const x = h.includes("w") ? r.x : h.includes("e") ? r.x + r.w : r.x + r.w / 2;
  const y = h.includes("n") ? r.y : h.includes("s") ? r.y + r.h : r.y + r.h / 2;
  return { left: x - 4, top: y - 4 };
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
  const { projectId, project, doc, selection, compare, guides, captures, busy } = ed;
  const [render, setRender] = useState<RenderResult | null>(null);
  const [originalSrc, setOriginalSrc] = useState<string>();
  const pointer = useCanvasPointer(viewportRef, view, setView, panes);
  const { drag, preview, clearPreview } = pointer;

  const W = doc?.artboard.width_mm ?? 1;
  const H = doc?.artboard.height_mm ?? 1;
  const z = view.zoom;
  const scale = renderScale(z);
  const rev = doc?.rev;
  const original = project?.originals.at(-1);
  const originalVersion = original?.version;

  useEffect(() => {
    if (rev === undefined) return;
    let alive = true;
    const t = window.setTimeout(() => {
      api
        .renderLayers(projectId, scale)
        .then((r) => {
          if (!alive) return;
          setRender(r);
          clearPreview();
        })
        .catch((e) => alive && toast(errorMessage(e), "error"));
    }, 150);
    return () => {
      alive = false;
      window.clearTimeout(t);
    };
  }, [projectId, scale, rev, clearPreview]);

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

  const sources = useMemo(
    () => new Map(render?.layers.map((r) => [r.id, pngSrc(r.png_base64)]) ?? []),
    [render],
  );

  if (!doc) return <div ref={viewportRef} className="cv-viewport" />;

  const live = drag?.kind === "box" ? { id: drag.id, box: drag.box } : preview;
  const shownBox = (id: string, fallback: Box) => (live?.id === id ? live.box : fallback);
  const selected = selection.map((id) => doc.layers.find((l) => l.id === id)).filter((l) => l !== undefined);
  const single =
    selected.length === 1 && selected[0].kind.type !== "group" && !selected[0].locked ? selected[0] : null;
  const rs = render?.scale_px_per_mm ?? scale;
  const bg = doc.artboard.background;
  const work = panes.work;
  const mm = (b: Box) => px({ x: b.x * z, y: b.y * z, w: b.w * z, h: b.h * z });

  return (
    <div ref={viewportRef} className="cv-viewport" style={{ cursor: pointer.cursor }} {...pointer.handlers}>
      {panes.original && (
        <>
          <div
            className="cv-label t-micro"
            style={{ left: panes.original.x, top: panes.original.y - LABEL_H + 4 }}
          >
            원본 · v{original?.version} · {original?.width_px} × {original?.height_px} px
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
        style={{ ...px(work), background: bg?.type === "solid" ? bg.color : "#FFFFFF" }}
      >
        {render?.layers.map((r) => {
          const l = doc.layers.find((x) => x.id === r.id);
          const src = sources.get(r.id);
          if (!src || (l && isEffectivelyHidden(doc.layers, l))) return null;
          let box: Box = { x: r.x_px / rs, y: r.y_px / rs, w: r.w_px / rs, h: r.h_px / rs };
          if (l && live?.id === l.id) {
            const b0 = boxOf(doc.layers, l);
            const sx = live.box.w / (b0.w || 1);
            const sy = live.box.h / (b0.h || 1);
            box = {
              x: live.box.x + (box.x - b0.x) * sx,
              y: live.box.y + (box.y - b0.y) * sy,
              w: box.w * sx,
              h: box.h * sy,
            };
          }
          return <img key={r.id} className="cv-raster" src={src} alt="" draggable={false} style={mm(box)} />;
        })}
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
          className={drag.mode === "capture" ? "cv-capture" : "cv-shape-draft"}
          style={px(onScreen(drag.pane === "work" ? work : (panes.original ?? work), drag.box, z))}
        />
      )}

      {selected.map((l) => (
        <div
          key={l.id}
          className="cv-sel"
          style={px(onScreen(work, shownBox(l.id, boxOf(doc.layers, l)), z))}
        />
      ))}
      {single &&
        (() => {
          const b = shownBox(single.id, boxOf(doc.layers, single));
          const r = onScreen(work, b, z);
          return (
            <>
              {HANDLES.map((h) => (
                <div key={h} data-handle={h} className={`cv-handle h-${h}`} style={handleStyle(r, h)} />
              ))}
              <div className="cv-size t-micro" style={{ left: r.x + r.w / 2, top: r.y + r.h + 6 }}>
                {Math.round(b.w)} × {Math.round(b.h)} mm
              </div>
            </>
          );
        })()}
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

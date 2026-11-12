import type { Layer } from "@/shared/api";
import { HANDLE_UV, HANDLES, pointOn, type Handle, type Frame } from "./geometry";
import { cropBox, cropFrame, cropMap, type Edges } from "./crop";
import { HANDLE_HALF_PX, LABEL_GAP_PX, type Rect } from "./viewport";

export function CropView({
  layer,
  p,
  edges,
  href,
  z,
  pane,
}: {
  layer: Layer;
  p: Frame;
  edges: Edges;
  href: string | null;
  z: number;
  pane: Rect;
}) {
  if (layer.kind.type !== "image") return null;
  const k = layer.kind;
  const m = cropMap(k, p.w, p.h);
  const box = cropBox(edges, m);
  const img = { x: m.ox, y: m.oy, w: k.natural_w_px * m.sx, h: k.natural_h_px * m.sy };
  const frame = cropFrame(p, box);
  const at = (h: Handle) => {
    const q = pointOn(frame, ...HANDLE_UV[h]);
    return { left: pane.x + q.x * z - HANDLE_HALF_PX, top: pane.y + q.y * z - HANDLE_HALF_PX };
  };
  const label = pointOn(frame, 0.5, 1);

  return (
    <>
      <svg className="cv-crop" style={{ left: pane.x, top: pane.y }} width={1} height={1}>
        <g transform={`scale(${z}) translate(${p.x} ${p.y}) rotate(${p.deg} ${p.w / 2} ${p.h / 2})`}>
          {href && (
            <image href={href} x={img.x} y={img.y} width={img.w} height={img.h} preserveAspectRatio="none" />
          )}
          <path
            className="cv-crop-shade"
            fillRule="evenodd"
            d={`M${img.x} ${img.y}h${img.w}v${img.h}h${-img.w}ZM${box.x} ${box.y}h${box.w}v${box.h}h${-box.w}Z`}
          />
          <rect className="cv-crop-rect" x={box.x} y={box.y} width={box.w} height={box.h} />
        </g>
      </svg>
      {HANDLES.map((h) => (
        <div key={h} data-crop={h} className={`cv-handle h-${h}`} style={at(h)} />
      ))}
      <div
        className="cv-size t-micro"
        style={{ left: pane.x + label.x * z, top: pane.y + label.y * z + LABEL_GAP_PX }}
      >
        자르기 {Math.round(edges[2] - edges[0])} × {Math.round(edges[3] - edges[1])} px
      </div>
    </>
  );
}

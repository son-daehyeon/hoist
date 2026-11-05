import type { Layer } from "@/shared/api";
import { HANDLE_UV, pointOn, type Handle, type Params } from "./drag";
import { cropBox, cropFrame, cropMap, type Edges } from "./crop";
import { HANDLES } from "./pointer";
import type { Rect } from "./viewport";

export function CropView({
  layer,
  p,
  edges,
  href,
  z,
  pane,
}: {
  layer: Layer;
  p: Params;
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
    return { left: pane.x + q.x * z - 4, top: pane.y + q.y * z - 4 };
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
      <div className="cv-size t-micro" style={{ left: pane.x + label.x * z, top: pane.y + label.y * z + 6 }}>
        자르기 {Math.round(edges[2] - edges[0])} × {Math.round(edges[3] - edges[1])} px
      </div>
    </>
  );
}

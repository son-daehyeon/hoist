import { aabb, HANDLE_UV, HANDLES, pointOn, type Frame } from "./geometry";
import { HANDLE_HALF_PX, LABEL_GAP_PX, type Rect } from "./viewport";

const KNOB_PX = 22;
const KNOB_HALF_PX = 5;

export function TransformHandles({ frame, pane, z }: { frame: Frame; pane: Rect; z: number }) {
  const screenAt = (u: number, v: number) => {
    const q = pointOn(frame, u, v);
    return { x: pane.x + q.x * z, y: pane.y + q.y * z };
  };
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
      <div
        data-handle="rotate"
        className="cv-knob"
        style={{ left: knob.x - KNOB_HALF_PX, top: knob.y - KNOB_HALF_PX }}
      />
      {HANDLES.map((h) => {
        const s = screenAt(...HANDLE_UV[h]);
        return (
          <div
            key={h}
            data-handle={h}
            className={`cv-handle h-${h}`}
            style={{ left: s.x - HANDLE_HALF_PX, top: s.y - HANDLE_HALF_PX }}
          />
        );
      })}
      <div
        className="cv-size t-micro"
        style={{ left: pane.x + (box.x + box.w / 2) * z, top: pane.y + (box.y + box.h) * z + LABEL_GAP_PX }}
      >
        {Math.round(frame.w)} × {Math.round(frame.h)} mm
        {Math.round(frame.deg) % 360 !== 0 ? ` · ${Math.round(frame.deg)}°` : ""}
      </div>
    </>
  );
}

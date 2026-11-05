import { mixHex, paintCss } from "../lib";
import type { GradientStop, Paint } from "../api";
import { IconButton } from "./Button";
import { ScrubField } from "./ScrubField";
import { track } from "./pointerTrack";

export type Gradient = Extract<Paint, { type: "linear_gradient" }>;

function colorAt(stops: GradientStop[], t: number): GradientStop {
  const sorted = [...stops].sort((a, b) => a.offset - b.offset);
  const after = sorted.find((s) => s.offset >= t) ?? sorted[sorted.length - 1];
  const before = [...sorted].reverse().find((s) => s.offset <= t) ?? sorted[0];
  const span = after.offset - before.offset;
  const k = span > 0 ? (t - before.offset) / span : 0;
  return {
    offset: t,
    color: mixHex(before.color, after.color, k),
    opacity: before.opacity + (after.opacity - before.opacity) * k,
  };
}

export function GradientEditor({
  paint,
  selected,
  onSelect,
  onDraft,
  onCommit,
}: {
  paint: Gradient;
  selected: number;
  onSelect: (i: number) => void;
  onDraft: (p: Gradient) => void;
  onCommit: (p: Gradient, select?: number) => void;
}) {
  const bar = { ...paint, angle_deg: 0 };
  const wrapRect = (el: HTMLElement) => (el.parentElement ?? el).getBoundingClientRect();
  const angleOf = (fx: number, fy: number, snap: boolean) => {
    const deg = (Math.atan2(fy - 0.5, fx - 0.5) * 180) / Math.PI;
    const step = snap ? 15 : 1;
    return (((Math.round(deg / step) * step) % 360) + 360) % 360;
  };

  return (
    <div className="bs-grad-wrap">
      <div className="bs-grad">
        <div
          className="bs-grad-bar bs-checker"
          title="눌러서 색 지점 더하기"
          onPointerDown={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            const t = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
            const stops = [...paint.stops, colorAt(paint.stops, Math.round(t * 1000) / 1000)];
            onCommit({ ...paint, stops }, stops.length - 1);
          }}
        >
          <span style={{ background: paintCss(bar) }} />
        </div>
        {paint.stops.map((s, i) => (
          <span
            key={i}
            role="slider"
            aria-label={`색 지점 ${i + 1}`}
            aria-valuenow={Math.round(s.offset * 100)}
            className={`bs-grad-stop ${i === selected ? "is-selected" : ""}`}
            style={{ left: `${s.offset * 100}%`, background: s.color }}
            {...track<HTMLSpanElement>(
              (fx) => {
                if (i !== selected) onSelect(i);
                onDraft({
                  ...paint,
                  stops: paint.stops.map((x, j) =>
                    j === i ? { ...x, offset: Math.round(fx * 1000) / 1000 } : x,
                  ),
                });
              },
              () => onCommit(paint, i),
              wrapRect,
            )}
          />
        ))}
      </div>
      <div className="bs-grad-row">
        <ScrubField
          label="각도"
          value={paint.angle_deg}
          min={0}
          max={360}
          decimals={0}
          suffix="°"
          onCommit={(v) => onCommit({ ...paint, angle_deg: v % 360 })}
        />
        <span
          className="bs-grad-dial"
          role="slider"
          aria-label="각도 돌리기"
          aria-valuenow={paint.angle_deg}
          title="돌려서 각도 바꾸기 (Shift: 15°씩)"
          {...track<HTMLSpanElement>(
            (fx, fy, e) => onDraft({ ...paint, angle_deg: angleOf(fx, fy, e.shiftKey) }),
            () => onCommit(paint),
          )}
        >
          <span style={{ transform: `rotate(${paint.angle_deg}deg)` }} />
        </span>
        <IconButton
          icon="minus"
          label="선택한 색 지점 빼기"
          disabled={paint.stops.length <= 2}
          onClick={() => {
            const stops = paint.stops.filter((_, j) => j !== selected);
            onCommit({ ...paint, stops }, Math.max(0, selected - 1));
          }}
        />
      </div>
    </div>
  );
}

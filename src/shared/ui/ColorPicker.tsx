import { useCallback, useState } from "react";
import { hexToHsv, hsvToHex, mixHex, normalizeHex, paintCss, rgba, type Hsv } from "../lib";
import type { Paint } from "../api";
import { IconButton } from "./Button";
import { GradientEditor, type Gradient } from "./GradientEditor";
import { Icon } from "./Icon";
import { Popover } from "./Popover";
import { ScrubField } from "./ScrubField";
import { Segmented } from "./Tabs";
import { Tooltip } from "./Tooltip";
import { track } from "./pointerTrack";
import "./ColorPicker.css";

const RECENT_KEY = "hoist.recent-colors";

function loadRecent(): string[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, 12) : [];
  } catch {
    return [];
  }
}

function startGradient(color: string, opacity: number): Gradient {
  return {
    type: "linear_gradient",
    angle_deg: 90,
    stops: [
      { offset: 0, color, opacity },
      { offset: 1, color: mixHex(color, "#FFFFFF", 0.6), opacity },
    ],
  };
}

function activeOf(p: Paint, stop: number) {
  if (p.type === "solid") return { color: p.color, opacity: p.opacity };
  const s = p.stops[Math.min(stop, p.stops.length - 1)];
  return { color: s.color, opacity: s.opacity };
}

function sortStops(p: Paint, select: number): [Paint, number] {
  if (p.type === "solid") return [p, 0];
  const chosen = p.stops[select];
  const stops = [...p.stops].sort((a, b) => a.offset - b.offset);
  return [{ ...p, stops }, Math.max(0, stops.indexOf(chosen))];
}

export function ColorSwatch({
  paint,
  label,
  onOpen,
  mixed = false,
}: {
  paint: Paint | null;
  label: string;
  onOpen: (el: HTMLButtonElement) => void;
  mixed?: boolean;
}) {
  return (
    <Tooltip label={label}>
      <button
        type="button"
        className="bs-swatch bs-checker"
        aria-label={label}
        onClick={(e) => onOpen(e.currentTarget)}
      >
        {mixed ? <Icon name="minus" size={14} /> : paint && <span style={{ background: paintCss(paint) }} />}
      </button>
    </Tooltip>
  );
}

function HexInput({ hex, onPick }: { hex: string; onPick: (hex: string) => void }) {
  const [draft, setDraft] = useState(hex);
  const [syncedFrom, setSyncedFrom] = useState(hex);
  if (syncedFrom !== hex) {
    setSyncedFrom(hex);
    setDraft(hex);
  }
  const commit = () => {
    const n = normalizeHex(draft);
    if (n && n !== hex) onPick(n);
    else setDraft(hex);
  };
  return (
    <label className="bs-scrub bs-hex">
      <span className="bs-scrub-label t-body-m">#</span>
      <input
        className="t-body-m"
        value={draft.replace("#", "")}
        maxLength={7}
        aria-label="색 코드"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
    </label>
  );
}

export function ColorPicker({
  anchor,
  value,
  onChange,
  onClose,
  title,
  allowGradient = false,
  palette = [],
  eyedropper,
}: {
  anchor: HTMLElement;
  value: Paint;
  onChange: (p: Paint) => void;
  onClose: () => void;
  title: string;
  allowGradient?: boolean;
  palette?: string[];
  eyedropper?: () => Promise<string | null>;
}) {
  const [draft, setDraft] = useState<Paint>(value);
  const [synced, setSynced] = useState<Paint>(value);
  const [stop, setStop] = useState(0);
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(activeOf(value, 0).color));
  const [dropping, setDropping] = useState(false);
  const [recent, setRecent] = useState(loadRecent);
  if (synced !== value) {
    setSynced(value);
    setDraft(value);
    if (value.type === "linear_gradient" && stop >= value.stops.length) setStop(value.stops.length - 1);
    const color = activeOf(value, stop).color;
    if (hsvToHex(hsv) !== color) setHsv(hexToHsv(color));
  }

  const active = activeOf(draft, stop);
  const withActive = (color: string, opacity: number): Paint =>
    draft.type === "solid"
      ? { ...draft, color, opacity }
      : { ...draft, stops: draft.stops.map((s, i) => (i === stop ? { ...s, color, opacity } : s)) };

  const commit = (p: Paint, select = stop) => {
    const [next, index] = sortStops(p, select);
    setDraft(next);
    setStop(index);
    const { color } = activeOf(next, index);
    const list = [color, ...recent.filter((c) => c !== color)].slice(0, 12);
    setRecent(list);
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
    onChange(next);
  };
  const pick = (hex: string) => {
    setHsv(hexToHsv(hex));
    commit(withActive(hex, active.opacity));
  };
  const select = (i: number) => {
    setStop(i);
    if (draft.type === "linear_gradient") setHsv(hexToHsv(draft.stops[i].color));
  };
  const setHsvDraft = (next: Hsv) => {
    setHsv(next);
    setDraft(withActive(hsvToHex(next), active.opacity));
  };
  const keepOpen = useCallback(() => dropping, [dropping]);

  const drop = async () => {
    if (!eyedropper) return;
    setDropping(true);
    const hex = await eyedropper();
    setDropping(false);
    if (hex) pick(hex);
  };

  const hue = hsvToHex({ h: hsv.h, s: 1, v: 1 });
  const swatches = (list: string[]) => (
    <div className="bs-cp-swatches">
      {list.map((c) => (
        <button
          key={c}
          type="button"
          className="bs-cp-chip"
          style={{ background: c }}
          title={c}
          aria-label={`${c} 고르기`}
          onClick={() => pick(c)}
        />
      ))}
    </div>
  );

  return (
    <Popover anchor={anchor} onClose={onClose} side="right" width={248} keepOpen={keepOpen} label={title}>
      <div className="bs-cp">
        <div className="bs-cp-head">
          {allowGradient ? (
            <Segmented
              value={draft.type}
              onChange={(t) => {
                if (t === draft.type) return;
                commit(
                  t === "solid" ? { type: "solid", ...active } : startGradient(active.color, active.opacity),
                  0,
                );
              }}
              options={[
                { value: "solid", label: "단색" },
                { value: "linear_gradient", label: "그라디언트" },
              ]}
            />
          ) : (
            <span className="t-label-m">{title}</span>
          )}
          {eyedropper && (
            <IconButton
              icon="eyedropper"
              label="스포이트 — 화면에서 색 집기"
              active={dropping}
              onClick={drop}
            />
          )}
        </div>
        {dropping && (
          <div className="t-body-m c-tertiary">원본이나 작업본을 누르면 그 색을 집어요. Esc는 취소</div>
        )}

        {draft.type === "linear_gradient" && (
          <GradientEditor
            paint={draft}
            selected={stop}
            onSelect={select}
            onDraft={setDraft}
            onCommit={commit}
          />
        )}

        <div
          className="bs-cp-sv"
          style={{ background: hue }}
          role="slider"
          aria-label="채도와 밝기"
          {...track<HTMLDivElement>(
            (fx, fy) => setHsvDraft({ h: hsv.h, s: fx, v: 1 - fy }),
            () => commit(draft),
          )}
        >
          <span className="bs-cp-thumb" style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%` }} />
        </div>
        <div
          className="bs-cp-slider bs-cp-hue"
          role="slider"
          aria-label="색상"
          aria-valuenow={Math.round(hsv.h)}
          {...track<HTMLDivElement>(
            (fx) => setHsvDraft({ ...hsv, h: Math.min(359.9, fx * 360) }),
            () => commit(draft),
          )}
        >
          <span className="bs-cp-thumb" style={{ left: `${(hsv.h / 360) * 100}%` }} />
        </div>
        <div
          className="bs-cp-slider bs-checker"
          role="slider"
          aria-label="불투명도"
          aria-valuenow={Math.round(active.opacity * 100)}
          {...track<HTMLDivElement>(
            (fx) => setDraft(withActive(active.color, Math.round(fx * 100) / 100)),
            () => commit(draft),
          )}
        >
          <span
            className="bs-cp-alpha"
            style={{
              background: `linear-gradient(90deg, ${rgba(active.color, 0)}, ${rgba(active.color, 1)})`,
            }}
          />
          <span className="bs-cp-thumb" style={{ left: `${active.opacity * 100}%` }} />
        </div>

        <div className="bs-cp-row">
          <HexInput hex={active.color} onPick={pick} />
          <ScrubField
            label="불투명"
            value={Math.round(active.opacity * 100)}
            min={0}
            max={100}
            decimals={0}
            suffix="%"
            onCommit={(v) => commit(withActive(active.color, v / 100))}
          />
        </div>

        {recent.length > 0 && (
          <div className="bs-cp-group">
            <span className="t-body-m c-tertiary">최근 쓴 색</span>
            {swatches(recent)}
          </div>
        )}
        {palette.length > 0 && (
          <div className="bs-cp-group">
            <span className="t-body-m c-tertiary">이 현수막에 쓴 색</span>
            {swatches(palette)}
          </div>
        )}
      </div>
    </Popover>
  );
}

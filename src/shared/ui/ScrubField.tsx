import { useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import "./ScrubField.css";
import { noAutofill } from "./noAutofill";

const format = (v: number, decimals: number) => String(Number(v.toFixed(decimals)));

export function ScrubField({
  label,
  value,
  onCommit,
  step = 1,
  min = -Infinity,
  max = Infinity,
  decimals = 1,
  suffix,
  title,
  disabled,
}: {
  label?: ReactNode;
  value: number | null;
  onCommit: (v: number) => void;
  step?: number;
  min?: number;
  max?: number;
  decimals?: number;
  suffix?: string;
  title?: string;
  disabled?: boolean;
}) {
  const shown = value === null ? "" : format(value, decimals);
  const [draft, setDraft] = useState(shown);
  const [syncedFrom, setSyncedFrom] = useState(shown);
  const [scrub, setScrub] = useState<{ x: number; from: number; moved: boolean } | null>(null);
  const cancelled = useRef(false);
  if (syncedFrom !== shown) {
    setSyncedFrom(shown);
    setDraft(shown);
  }

  const clamp = (v: number) => Math.min(max, Math.max(min, v));
  const parsed = () => {
    const n = Number(draft.replace(",", "."));
    return draft.trim() !== "" && Number.isFinite(n) ? clamp(n) : null;
  };
  const commit = () => {
    const n = parsed();
    if (n === null || format(n, decimals) === shown) return setDraft(shown);
    setDraft(format(n, decimals));
    onCommit(Number(format(n, decimals)));
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") e.currentTarget.blur();
    else if (e.key === "Escape") {
      setDraft(shown);
      cancelled.current = true;
      e.currentTarget.blur();
    } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const base = parsed() ?? value ?? 0;
      const delta = (e.key === "ArrowUp" ? 1 : -1) * step * (e.shiftKey ? 10 : 1);
      setDraft(format(clamp(base + delta), decimals));
    }
  };

  const grab = {
    onPointerDown: (e: PointerEvent<HTMLSpanElement>) => {
      if (disabled || e.button !== 0) return;
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      setScrub({ x: e.clientX, from: parsed() ?? value ?? 0, moved: false });
    },
    onPointerMove: (e: PointerEvent<HTMLSpanElement>) => {
      if (!scrub) return;
      const dx = e.clientX - scrub.x;
      if (!scrub.moved && Math.abs(dx) < 2) return;
      if (!scrub.moved) setScrub({ ...scrub, moved: true });
      setDraft(format(clamp(scrub.from + Math.round(dx) * step * (e.shiftKey ? 10 : 1)), decimals));
    },
    onPointerUp: () => {
      if (!scrub) return;
      setScrub(null);
      if (scrub.moved) commit();
    },
  };

  return (
    <label className={`bs-scrub ${scrub?.moved ? "is-scrubbing" : ""}`} title={title}>
      {label === undefined ? (
        <span className="bs-scrub-pad" />
      ) : (
        <span className="bs-scrub-label t-body-m" {...grab}>
          {label}
        </span>
      )}
      <input
        {...noAutofill}
        className="t-body-m"
        value={draft}
        placeholder={value === null ? "혼합" : undefined}
        disabled={disabled}
        inputMode="decimal"
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          if (cancelled.current) cancelled.current = false;
          else commit();
        }}
        onKeyDown={onKeyDown}
      />
      {suffix && <span className="bs-scrub-suffix t-body-m">{suffix}</span>}
    </label>
  );
}

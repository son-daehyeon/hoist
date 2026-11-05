import { useRef, useState } from "react";
import { ScrubField } from "./ScrubField";
import "./SliderField.css";
import { noAutofill } from "./noAutofill";

export function SliderField({
  label,
  value,
  onCommit,
  min,
  max,
  step = 1,
  suffix,
  title,
}: {
  label: string;
  value: number;
  onCommit: (v: number) => void;
  min: number;
  max: number;
  step?: number;
  suffix?: string;
  title?: string;
}) {
  const [draft, setDraft] = useState(value);
  const [syncedFrom, setSyncedFrom] = useState(value);
  const latest = useRef(value);
  if (syncedFrom !== value) {
    setSyncedFrom(value);
    setDraft(value);
  }

  const commit = (v: number) => {
    const c = Math.min(max, Math.max(min, v));
    setDraft(c);
    if (c !== value) onCommit(c);
  };

  return (
    <div className="bs-slider" title={title}>
      <span className="t-body-m bs-slider-label">{label}</span>
      <input
        {...noAutofill}
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={draft}
        onChange={(e) => {
          latest.current = Number(e.target.value);
          setDraft(latest.current);
        }}
        onPointerDown={() => {
          latest.current = draft;
          window.addEventListener("pointerup", () => commit(latest.current), { once: true });
        }}
        onKeyUp={() => commit(latest.current)}
      />
      <ScrubField
        value={draft}
        min={min}
        max={max}
        step={step}
        decimals={0}
        suffix={suffix}
        onCommit={commit}
      />
    </div>
  );
}

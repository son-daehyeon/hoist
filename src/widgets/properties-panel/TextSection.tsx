import { useState } from "react";
import { useEd } from "@/entities/editor";
import type { Layer, TextProps } from "@/shared/api";
import { Checkbox, Icon, IconButton, Popover, ScrubField, Select, SliderField } from "@/shared/ui";
import { updateAll } from "./model";
import { PropSection } from "./Section";

const WEIGHTS: [number, string][] = [
  [100, "아주 가늘게"],
  [200, "더 가늘게"],
  [300, "가늘게"],
  [400, "보통"],
  [500, "중간"],
  [600, "약간 굵게"],
  [700, "굵게"],
  [800, "더 굵게"],
  [900, "아주 굵게"],
];

const ALIGNS: { value: TextProps["align"]; icon: string; label: string }[] = [
  { value: "left", icon: "align-left", label: "왼쪽 맞춤" },
  { value: "center", icon: "align-center", label: "가운데 맞춤" },
  { value: "right", icon: "align-right", label: "오른쪽 맞춤" },
];

function FontPicker({
  family,
  families,
  onPick,
}: {
  family: string;
  families: string[];
  onPick: (f: string) => void;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <>
      <button
        type="button"
        className="pp-font t-body-m"
        aria-haspopup="listbox"
        onClick={(e) => setAnchor(anchor ? null : e.currentTarget)}
      >
        <span style={{ fontFamily: `"${family}"` }}>{family}</span>
        <Icon name="chevron-down" size={14} />
      </button>
      {anchor && (
        <Popover anchor={anchor} onClose={() => setAnchor(null)} width={anchor.offsetWidth} label="글꼴">
          <div className="pp-font-list" role="listbox" aria-label="글꼴">
            {families.map((f) => (
              <button
                key={f}
                type="button"
                role="option"
                aria-selected={f === family}
                className="pp-font-item"
                onClick={() => {
                  setAnchor(null);
                  if (f !== family) onPick(f);
                }}
              >
                <span className="pp-font-check">{f === family && <Icon name="check" size={14} />}</span>
                <span style={{ fontFamily: `"${f}"` }}>{f}</span>
              </button>
            ))}
          </div>
        </Popover>
      )}
    </>
  );
}

export function TextSection({ layer, families }: { layer: Layer; families: string[] }) {
  const { ops } = useEd();
  if (layer.kind.type !== "text") return null;
  const k = layer.kind;
  const set = (patch: Partial<TextProps>) => updateAll(ops, [layer], () => ({ kind: patch }));
  const weights = WEIGHTS.some(([w]) => w === k.font_weight)
    ? WEIGHTS
    : [...WEIGHTS, [k.font_weight, ""] as [number, string]];

  return (
    <PropSection id="text" title="글자">
      <FontPicker
        family={k.font_family}
        families={families.includes(k.font_family) ? families : [k.font_family, ...families]}
        onPick={(font_family) => set({ font_family })}
      />
      <div className="pp-grid2">
        <ScrubField
          label="크기"
          title="글자 크기 (mm)"
          value={k.font_size_mm}
          min={0.5}
          suffix="mm"
          onCommit={(font_size_mm) => set({ font_size_mm })}
        />
        <Select
          aria-label="굵기"
          value={String(k.font_weight)}
          onChange={(e) => set({ font_weight: Number(e.target.value) })}
          options={weights.map(([w, name]) => ({
            value: String(w),
            label: name ? `${w} ${name}` : String(w),
          }))}
        />
        <ScrubField
          label="자간"
          title="글자 사이 간격 (mm)"
          value={k.letter_spacing_mm}
          step={0.1}
          suffix="mm"
          onCommit={(letter_spacing_mm) => set({ letter_spacing_mm })}
        />
        <ScrubField
          label="행간"
          title="줄 간격 (글자 크기의 배수)"
          value={k.line_height}
          step={0.05}
          min={0.5}
          decimals={2}
          suffix="배"
          onCommit={(line_height) => set({ line_height })}
        />
      </div>
      <div className="pp-row">
        <span className="t-body-m c-secondary">정렬</span>
        <span className="pp-spacer" />
        {ALIGNS.map((a) => (
          <IconButton
            key={a.value}
            icon={a.icon}
            label={a.label}
            active={k.align === a.value}
            onClick={() => k.align !== a.value && set({ align: a.value })}
          />
        ))}
      </div>
      <SliderField
        label="장평"
        title="글자 가로 폭 (보통 100%)"
        value={Math.round(k.scale_x * 100)}
        min={50}
        max={150}
        suffix="%"
        onCommit={(v) => set({ scale_x: v / 100 })}
      />
      <SliderField
        label="기울임"
        title="오른쪽으로 기울이기 (°), 음수는 왼쪽"
        value={Math.round(k.skew_deg)}
        min={-60}
        max={60}
        suffix="°"
        onCommit={(skew_deg) => set({ skew_deg })}
      />
      <SliderField
        label="아치"
        title="위로 볼록(+), 아래로 볼록(−). 켜면 가운데 맞춤으로 바뀌어요"
        value={Math.round(k.arc_bend * 100)}
        min={-100}
        max={100}
        suffix="%"
        onCommit={(v) =>
          set(
            v !== 0 && k.align !== "center" ? { arc_bend: v / 100, align: "center" } : { arc_bend: v / 100 },
          )
        }
      />
      <Checkbox checked={k.vertical} onChange={(vertical) => set({ vertical })}>
        세로쓰기
      </Checkbox>
    </PropSection>
  );
}

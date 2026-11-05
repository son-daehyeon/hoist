import { useEd } from "@/entities/editor";
import type { Layer, Paint, Stroke } from "@/shared/api";
import { IconButton, ScrubField, Select } from "@/shared/ui";
import { moveItem, paintLabel, solid, updateAll } from "./model";
import { PaintField, PropSection } from "./Section";

const ALIGN_OPTIONS = [
  { value: "inside", label: "안쪽" },
  { value: "center", label: "가운데" },
  { value: "outside", label: "바깥쪽" },
];
const JOIN_OPTIONS = [
  { value: "miter", label: "각지게" },
  { value: "round", label: "둥글게" },
  { value: "bevel", label: "깎아서" },
];

export function FillSection({ layer, palette }: { layer: Layer; palette: string[] }) {
  const { ops } = useEd();
  const save = (fills: Paint[]) => updateAll(ops, [layer], () => ({ fills }));
  const replace = (i: number, p: Paint) => save(layer.fills.map((f, j) => (j === i ? p : f)));

  return (
    <PropSection
      id="fills"
      title="채우기"
      action={
        <IconButton
          icon="plus"
          label="채우기 더하기"
          onClick={() => save([...layer.fills, solid("#D9D9D9")])}
        />
      }
    >
      {layer.fills.length === 0 && <div className="t-body-m c-tertiary">채우기 없음</div>}
      {layer.fills.map((f, i) => (
        <div key={i} className="pp-paint-row">
          <PaintField
            paint={f}
            label={`채우기 ${i + 1} 색`}
            palette={palette}
            onChange={(p) => replace(i, p)}
          />
          <span className={`t-body-m pp-paint-name ${f.type === "solid" ? "" : "is-wide"}`}>
            {paintLabel(f)}
          </span>
          {f.type === "solid" && (
            <ScrubField
              label="불투명"
              title="불투명도 (%)"
              value={Math.round(f.opacity * 100)}
              min={0}
              max={100}
              decimals={0}
              onCommit={(v) => replace(i, { ...f, opacity: v / 100 })}
            />
          )}
          <IconButton
            icon="minus"
            label="이 채우기 빼기"
            onClick={() => save(layer.fills.filter((_, j) => j !== i))}
          />
        </div>
      ))}
    </PropSection>
  );
}

export function StrokeSection({ layer, palette }: { layer: Layer; palette: string[] }) {
  const { ops } = useEd();
  const save = (strokes: Stroke[]) => updateAll(ops, [layer], () => ({ strokes }));
  const edit = (i: number, patch: Partial<Stroke>) =>
    save(layer.strokes.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  const fresh: Stroke = {
    paint: solid("#000000"),
    width_mm: Math.max(1, Math.round(Math.min(layer.w_mm, layer.h_mm) * 0.02)),
    align: "outside",
    join: "round",
    dash_mm: [],
  };
  const many = layer.strokes.length > 1;

  return (
    <PropSection
      id="strokes"
      title="외곽선"
      action={
        <IconButton icon="plus" label="외곽선 더하기" onClick={() => save([...layer.strokes, fresh])} />
      }
    >
      {layer.strokes.length === 0 && <div className="t-body-m c-tertiary">외곽선 없음</div>}
      {layer.strokes.map((s, i) => (
        <div key={i} className="pp-item">
          <div className="pp-paint-row">
            <PaintField
              paint={s.paint}
              label={`외곽선 ${i + 1} 색`}
              palette={palette}
              onChange={(paint) => edit(i, { paint })}
            />
            <span className="t-body-m pp-paint-name">{paintLabel(s.paint)}</span>
            <ScrubField
              label="두께"
              title="외곽선 두께 (mm)"
              value={s.width_mm}
              min={0}
              onCommit={(width_mm) => edit(i, { width_mm })}
            />
            <IconButton
              icon="minus"
              label="이 외곽선 빼기"
              onClick={() => save(layer.strokes.filter((_, j) => j !== i))}
            />
          </div>
          <div className="pp-grid2">
            <Select
              aria-label="외곽선 위치"
              value={s.align}
              options={ALIGN_OPTIONS}
              onChange={(e) => edit(i, { align: e.target.value as Stroke["align"] })}
            />
            <Select
              aria-label="모서리 모양"
              value={s.join}
              options={JOIN_OPTIONS}
              onChange={(e) => edit(i, { join: e.target.value as Stroke["join"] })}
            />
          </div>
          <div className={many ? "pp-dash-row" : ""}>
            <ScrubField
              label="점선"
              title="점선 길이 (mm), 0이면 실선"
              value={s.dash_mm[0] ?? 0}
              min={0}
              suffix="mm"
              onCommit={(d) => edit(i, { dash_mm: d > 0 ? [d, d] : [] })}
            />
            {many && (
              <>
                <IconButton
                  icon="chevron-up"
                  label="위로 올리기"
                  disabled={i === 0}
                  onClick={() => save(moveItem(layer.strokes, i, i - 1))}
                />
                <IconButton
                  icon="chevron-down"
                  label="아래로 내리기"
                  disabled={i === layer.strokes.length - 1}
                  onClick={() => save(moveItem(layer.strokes, i, i + 1))}
                />
              </>
            )}
          </div>
        </div>
      ))}
    </PropSection>
  );
}

import { useState } from "react";
import { useEd } from "@/entities/editor";
import { api, errorMessage, type ImageProps, type Layer, type ShapeProps } from "@/shared/api";
import { Button, Checkbox, ScrubField, Segmented, Select, SliderField, toast } from "@/shared/ui";
import { updateAll, type Patch } from "./model";
import { PropSection } from "./Section";

const ADJUST = [
  ["brightness", "밝기"],
  ["contrast", "대비"],
  ["saturation", "채도"],
] as const;

const SHAPES: { value: ShapeProps["shape"]; label: string; make: () => ShapeProps }[] = [
  { value: "rect", label: "사각형", make: () => ({ shape: "rect", rx_mm: 0 }) },
  { value: "ellipse", label: "원", make: () => ({ shape: "ellipse" }) },
  { value: "line", label: "선", make: () => ({ shape: "line", arrow_start: false, arrow_end: false }) },
  { value: "polygon", label: "다각형", make: () => ({ shape: "polygon", sides: 6 }) },
  { value: "star", label: "별", make: () => ({ shape: "star", points: 5, inner_ratio: 0.5 }) },
];

export function ImageSection({ layer }: { layer: Layer }) {
  const { ops, projectId, locked } = useEd();
  const [cutting, setCutting] = useState(false);
  if (layer.kind.type !== "image") return null;
  const { fit, adjust, crop_px, asset } = layer.kind;
  const set = (patch: Partial<ImageProps>) => updateAll(ops, [layer], () => ({ kind: patch }));
  const removeBackground = async () => {
    setCutting(true);
    try {
      const r = await api.removeBackground(projectId, asset);
      set({ asset: r.asset, natural_w_px: r.width_px, natural_h_px: r.height_px });
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setCutting(false);
    }
  };
  return (
    <PropSection id="image" title="그림">
      <Segmented<ImageProps["fit"]>
        value={fit}
        onChange={(fit) => set({ fit })}
        options={[
          { value: "contain", label: "맞추기" },
          { value: "cover", label: "채우기" },
          { value: "fill", label: "늘이기" },
        ]}
      />
      <Button
        icon={cutting ? "loader" : "sparkles"}
        className={cutting ? "pp-busy" : ""}
        title="사람이나 물건만 남기고 뒤 배경을 투명하게 지워요 (몇 초 걸려요)"
        disabled={cutting || locked}
        onClick={() => void removeBackground()}
      >
        {cutting ? "배경 지우는 중…" : "배경 지우기"}
      </Button>
      {crop_px && (
        <Button icon="undo" onClick={() => set({ crop_px: null })}>
          자르기 되돌리기
        </Button>
      )}
      {ADJUST.map(([key, label]) => (
        <SliderField
          key={key}
          label={label}
          title={`${label} (0이면 원래대로)`}
          value={Math.round(adjust[key] * 100)}
          min={-100}
          max={100}
          suffix="%"
          onCommit={(v) => set({ adjust: { ...adjust, [key]: v / 100 } })}
        />
      ))}
    </PropSection>
  );
}

export function ShapeSection({ layer }: { layer: Layer }) {
  const { ops } = useEd();
  if (layer.kind.type !== "shape") return null;
  const k = layer.kind;
  const set = (patch: Patch) => updateAll(ops, [layer], () => ({ kind: patch }));
  const switchTo = (shape: string) => {
    const fresh = SHAPES.find((s) => s.value === shape)?.make();
    if (!fresh || shape === k.shape) return;
    const stale = Object.keys(k).filter((key) => key !== "type" && !(key in fresh));
    set({ ...Object.fromEntries(stale.map((key): [string, null] => [key, null])), type: "shape", ...fresh });
  };
  return (
    <PropSection id="shape" title="도형">
      <Select
        aria-label="도형 모양"
        value={k.shape}
        options={SHAPES.map(({ value, label }) => ({ value, label }))}
        onChange={(e) => switchTo(e.target.value)}
      />
      {k.shape === "rect" && (
        <ScrubField
          label="둥근 모서리"
          title="모서리를 둥글게 깎는 반지름 (mm)"
          value={k.rx_mm}
          min={0}
          max={Math.min(layer.w_mm, layer.h_mm) / 2}
          suffix="mm"
          onCommit={(rx_mm) => set({ rx_mm })}
        />
      )}
      {k.shape === "polygon" && (
        <ScrubField
          label="변"
          title="변의 수 (3~64)"
          value={k.sides}
          min={3}
          max={64}
          decimals={0}
          onCommit={(sides) => set({ sides })}
        />
      )}
      {k.shape === "star" && (
        <div className="pp-grid2">
          <ScrubField
            label="꼭짓점"
            title="꼭짓점 수 (3~64)"
            value={k.points}
            min={3}
            max={64}
            decimals={0}
            onCommit={(points) => set({ points })}
          />
          <ScrubField
            label="안쪽"
            title="안쪽 꼭짓점 길이 (바깥 대비 5~95%)"
            value={Math.round(k.inner_ratio * 100)}
            min={5}
            max={95}
            decimals={0}
            suffix="%"
            onCommit={(v) => set({ inner_ratio: v / 100 })}
          />
        </div>
      )}
      {k.shape === "line" && (
        <div className="pp-row is-wrap">
          <Checkbox checked={k.arrow_start} onChange={(arrow_start) => set({ arrow_start })}>
            시작 화살표
          </Checkbox>
          <Checkbox checked={k.arrow_end} onChange={(arrow_end) => set({ arrow_end })}>
            끝 화살표
          </Checkbox>
        </div>
      )}
    </PropSection>
  );
}

import { useState } from "react";
import { useEd } from "@/entities/editor";
import type { Effect, Layer } from "@/shared/api";
import { IconButton, Menu, ScrubField } from "@/shared/ui";
import { solid, updateAll } from "./model";
import { PaintField, PropSection } from "./Section";

const SHADOW_FIELDS = [
  ["x_mm", "X", "오른쪽으로 밀기 (mm)"],
  ["y_mm", "Y", "아래로 밀기 (mm)"],
  ["blur_mm", "흐림", "번지는 정도 (mm)"],
  ["spread_mm", "퍼짐", "그림자 키우기 (mm)"],
] as const;

export function EffectSection({ layer, palette }: { layer: Layer; palette: string[] }) {
  const { ops } = useEd();
  const [menu, setMenu] = useState<DOMRect | null>(null);
  const save = (effects: Effect[]) => updateAll(ops, [layer], () => ({ effects }));
  const edit = (i: number, e: Effect) => save(layer.effects.map((x, j) => (j === i ? e : x)));
  const unit = Math.max(1, Math.round(Math.min(layer.w_mm, layer.h_mm) * 0.03));
  const remove = (i: number, what: string) => (
    <IconButton
      icon="minus"
      label={`이 ${what} 빼기`}
      onClick={() => save(layer.effects.filter((_, j) => j !== i))}
    />
  );

  return (
    <PropSection
      id="effects"
      title="효과"
      action={
        <IconButton
          icon="plus"
          label="효과 더하기"
          onClick={(e) => setMenu(e.currentTarget.getBoundingClientRect())}
        />
      }
    >
      {layer.effects.length === 0 && <div className="t-body-m c-tertiary">효과 없음</div>}
      {layer.effects.map((fx, i) =>
        fx.type === "drop_shadow" ? (
          <div key={i} className="pp-item">
            <div className="pp-paint-row is-compact">
              <PaintField
                paint={solid(fx.color, fx.opacity)}
                label="그림자 색"
                palette={palette}
                allowGradient={false}
                onChange={(p) => p.type === "solid" && edit(i, { ...fx, color: p.color, opacity: p.opacity })}
              />
              <span className="t-body-m pp-paint-name">그림자</span>
              {remove(i, "그림자")}
            </div>
            <div className="pp-grid2">
              {SHADOW_FIELDS.map(([key, label, title]) => (
                <ScrubField
                  key={key}
                  label={label}
                  title={title}
                  value={fx[key]}
                  min={key === "blur_mm" ? 0 : undefined}
                  onCommit={(v) => edit(i, { ...fx, [key]: v })}
                />
              ))}
            </div>
          </div>
        ) : (
          <div key={i} className="pp-paint-row is-compact">
            <span className="t-body-m pp-paint-name">흐림</span>
            <ScrubField
              label="반경"
              title="흐리게 하는 정도 (mm)"
              value={fx.radius_mm}
              min={0}
              suffix="mm"
              onCommit={(radius_mm) => edit(i, { ...fx, radius_mm })}
            />
            {remove(i, "흐림")}
          </div>
        ),
      )}
      {menu && (
        <Menu
          anchor={menu}
          align="end"
          onClose={() => setMenu(null)}
          items={[
            {
              label: "그림자 더하기",
              onSelect: () =>
                save([
                  ...layer.effects,
                  {
                    type: "drop_shadow",
                    x_mm: unit,
                    y_mm: unit,
                    blur_mm: unit * 2,
                    spread_mm: 0,
                    color: "#000000",
                    opacity: 0.3,
                  },
                ]),
            },
            {
              label: "흐림 더하기",
              onSelect: () => save([...layer.effects, { type: "layer_blur", radius_mm: unit }]),
            },
          ]}
        />
      )}
    </PropSection>
  );
}

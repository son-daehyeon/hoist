import { useRef, useState } from "react";
import { layerIcon, STRATEGY_LONG } from "../lib/format";
import { childrenOf } from "../lib/tree";
import type { Effect, Layer, Strategy, Stroke } from "../types";
import {
  Icon,
  IconButton,
  Menu,
  ToolbarDivider,
  ToolbarDropdown,
  ToolbarField,
  ToolbarSwatch,
  type MenuItem,
} from "../ui";
import { useEd } from "./state";

type MenuKind = "strategy" | "stroke" | "shadow" | "guides" | "compare" | "more";
const STROKES = [0, 2, 4, 6, 10];

export function ContextToolbar({ onFit, onReplaceOriginal }: { onFit: () => void; onReplaceOriginal: () => void }) {
  const { doc, selection, locked, guides, setGuides, compare, setCompare, ops } = useEd();
  const [menu, setMenu] = useState<{ kind: MenuKind; anchor: DOMRect } | null>(null);
  const [draftColor, setDraftColor] = useState<string | null>(null);
  const colorTimer = useRef(0);
  const open = (kind: MenuKind) => (anchor: DOMRect) => setMenu({ kind, anchor });

  if (locked) {
    return (
      <div className="cv-toolbar">
        <Icon name="lock" size={14} className="c-tertiary" />
        <span className="t-caption c-tertiary">도우미가 작업하는 동안 편집이 잠겨요</span>
      </div>
    );
  }
  if (!doc) return <div className="cv-toolbar" />;

  const layer: Layer | undefined = selection.length === 1 ? doc.layers.find((l) => l.id === selection[0]) : undefined;
  const patch = (p: Record<string, unknown>) => layer && void ops([{ op: "update_layer", id: layer.id, patch: p }]);
  const remove = () => void ops(selection.map((id) => ({ op: "delete_layer" as const, id })));
  const fill = layer?.fills[0];
  const stroke = layer?.strokes[0];
  const shadow = layer?.effects.some((e) => e.type === "drop_shadow") ?? false;
  const H = doc.artboard.height_mm;

  const toFront = (front: boolean) => {
    if (!layer) return;
    const parent = layer.parent ?? null;
    const n = childrenOf(doc.layers, parent).length - 1;
    void ops([{ op: "move_layer", id: layer.id, parent, index: front ? n : 0 }]);
  };

  const items = (kind: MenuKind): MenuItem[] => {
    switch (kind) {
      case "strategy":
        return (Object.keys(STRATEGY_LONG) as Strategy[]).map((s) => ({
          label: STRATEGY_LONG[s],
          checked: layer?.strategy === s,
          onSelect: () => patch({ strategy: s }),
        }));
      case "stroke":
        return STROKES.map((w) => ({
          label: w ? `${w}mm` : "없음",
          checked: (stroke?.width_mm ?? 0) === w,
          onSelect: () => {
            const next: Stroke[] = w
              ? [{ paint: stroke?.paint ?? { type: "solid", color: "#FFFFFF", opacity: 1 }, width_mm: w, align: "outside", join: "round", dash_mm: [] }]
              : [];
            patch({ strokes: next });
          },
        }));
      case "shadow":
        return [false, true].map((on) => ({
          label: on ? "그림자 넣기" : "없음",
          checked: shadow === on,
          onSelect: () => {
            const rest = layer?.effects.filter((e) => e.type !== "drop_shadow") ?? [];
            const add: Effect[] = on
              ? [{ type: "drop_shadow", x_mm: H * 0.004, y_mm: H * 0.006, blur_mm: H * 0.012, spread_mm: 0, color: "#000000", opacity: 0.25 }]
              : [];
            patch({ effects: [...rest, ...add] });
          },
        }));
      case "guides":
        return [{ label: "재단선 · 안전 영역 보기", checked: guides, onSelect: () => setGuides(!guides) }];
      case "compare":
        return [0, 0.25, 0.5, 0.75].map((v) => ({
          label: v ? `${v * 100}%` : "끄기",
          checked: compare === v,
          onSelect: () => setCompare(v),
        }));
      case "more":
        return selection.length
          ? [
              ...(layer
                ? [
                    { label: "맨 앞으로", icon: "chevron-up" as const, onSelect: () => toFront(true) },
                    { label: "맨 뒤로", icon: "chevron-down" as const, onSelect: () => toFront(false) },
                    "divider" as const,
                  ]
                : []),
              { label: "삭제", icon: "trash", shortcut: "Delete", danger: true, onSelect: remove },
            ]
          : [
              { label: "화면에 맞추기", icon: "fit", onSelect: onFit },
              { label: "원본 교체", icon: "swap", onSelect: onReplaceOriginal },
            ];
    }
  };

  return (
    <div className="cv-toolbar">
      {!layer && selection.length === 0 && (
        <>
          <span className="t-caption c-tertiary">선택된 레이어 없음</span>
          <ToolbarDivider />
          <ToolbarDropdown icon="ruler" label="재단선 · 안전 영역" onOpen={open("guides")} />
          <ToolbarDropdown icon="compare" label="원본 겹쳐 보기" onOpen={open("compare")} />
        </>
      )}
      {!layer && selection.length > 1 && <span className="t-caption c-secondary">레이어 {selection.length}개 선택</span>}
      {layer && (
        <>
          <ToolbarDropdown icon={layerIcon(layer)} label={STRATEGY_LONG[layer.strategy]} onOpen={open("strategy")} />
          {fill?.type === "solid" && (
            <>
              <ToolbarDivider />
              <ToolbarSwatch
                color={draftColor ?? fill.color}
                onChange={(hex) => {
                  setDraftColor(hex);
                  window.clearTimeout(colorTimer.current);
                  colorTimer.current = window.setTimeout(() => {
                    setDraftColor(null);
                    patch({ fills: [{ ...fill, color: hex }, ...layer.fills.slice(1)] });
                  }, 300);
                }}
              />
            </>
          )}
          <ToolbarDivider />
          <ToolbarDropdown icon="square" label={stroke ? `외곽선 ${stroke.width_mm}mm` : "외곽선 없음"} onOpen={open("stroke")} />
          <ToolbarDropdown icon="sliders" label={shadow ? "그림자 있음" : "그림자"} onOpen={open("shadow")} />
          <ToolbarDivider />
          <ToolbarField prefix="X" value={layer.x_mm} onCommit={(v) => patch({ x_mm: v })} />
          <ToolbarField prefix="Y" value={layer.y_mm} width={62} onCommit={(v) => patch({ y_mm: v })} />
          <ToolbarField prefix="W" value={layer.w_mm} onCommit={(v) => v > 0 && patch({ w_mm: v })} />
          <ToolbarField prefix="H" value={layer.h_mm} width={62} onCommit={(v) => v > 0 && patch({ h_mm: v })} />
          <span className="t-micro c-tertiary">mm</span>
        </>
      )}
      <span className="cv-toolbar-spacer" />
      {layer && (
        <IconButton
          icon={layer.locked ? "lock" : "unlock"}
          label={layer.locked ? "잠금 풀기" : "잠그기"}
          active={layer.locked}
          onClick={() => patch({ locked: !layer.locked })}
        />
      )}
      <IconButton icon="more" label="더 보기" onClick={(e) => open("more")(e.currentTarget.getBoundingClientRect())} />
      {menu && (
        <Menu
          anchor={menu.anchor}
          align={menu.kind === "more" ? "end" : "start"}
          items={items(menu.kind)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

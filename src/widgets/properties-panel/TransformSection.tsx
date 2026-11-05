import { useEd } from "@/entities/editor";
import type { Document, Layer } from "@/shared/api";
import { parentOffset } from "@/shared/lib";
import { ScrubField } from "@/shared/ui";
import { common, updateAll } from "./model";
import { PropSection } from "./Section";

export function TransformSection({ layers, doc }: { layers: Layer[]; doc: Document }) {
  const { ops } = useEd();
  const placed = layers.map((l) => ({ l, off: parentOffset(doc.layers, l) }));
  const offOf = (l: Layer) => placed.find((p) => p.l === l)?.off ?? { x: 0, y: 0 };
  const hasGroup = layers.some((l) => l.kind.type === "group");
  const set = (fn: (l: Layer) => Record<string, number>) => updateAll(ops, layers, fn);

  return (
    <PropSection id="transform" title="위치와 크기 (mm)">
      <div className="pp-grid2">
        <ScrubField
          label="X"
          title="왼쪽 끝 위치"
          value={common(placed, (p) => p.off.x + p.l.x_mm)}
          onCommit={(v) => set((l) => ({ x_mm: v - offOf(l).x }))}
        />
        <ScrubField
          label="Y"
          title="위쪽 끝 위치"
          value={common(placed, (p) => p.off.y + p.l.y_mm)}
          onCommit={(v) => set((l) => ({ y_mm: v - offOf(l).y }))}
        />
        <ScrubField
          label="가로"
          value={common(layers, (l) => l.w_mm)}
          min={0.1}
          disabled={hasGroup}
          onCommit={(v) => set(() => ({ w_mm: v }))}
        />
        <ScrubField
          label="세로"
          value={common(layers, (l) => l.h_mm)}
          min={0.1}
          disabled={hasGroup}
          onCommit={(v) => set(() => ({ h_mm: v }))}
        />
        {layers.length === 1 && (
          <ScrubField
            label="회전"
            value={layers[0].rotation_deg}
            min={-360}
            max={360}
            suffix="°"
            onCommit={(v) => set(() => ({ rotation_deg: v }))}
          />
        )}
        <ScrubField
          label="불투명"
          value={common(layers, (l) => Math.round(l.opacity * 100))}
          min={0}
          max={100}
          decimals={0}
          suffix="%"
          onCommit={(v) => set(() => ({ opacity: v / 100 }))}
        />
      </div>
    </PropSection>
  );
}

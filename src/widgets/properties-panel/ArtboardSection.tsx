import { useEd } from "@/entities/editor";
import type { Document, Paint } from "@/shared/api";
import { ScrubField } from "@/shared/ui";
import { paintLabel } from "./model";
import { PaintField, PropSection } from "./Section";

export function ArtboardSection({ doc, palette }: { doc: Document; palette: string[] }) {
  const { ops } = useEd();
  const { width_mm, height_mm, bleed_mm, background } = doc.artboard;
  const set = (patch: { width_mm?: number; height_mm?: number; bleed_mm?: number; background?: Paint }) =>
    void ops([{ op: "set_artboard", patch }]);
  return (
    <PropSection id="artboard" title="크기 (mm)">
      <div className="pp-grid2">
        <ScrubField label="가로" value={width_mm} min={1} onCommit={(v) => set({ width_mm: v })} />
        <ScrubField label="세로" value={height_mm} min={1} onCommit={(v) => set({ height_mm: v })} />
        <ScrubField
          label="재단"
          title="재단 여백 (mm)"
          value={bleed_mm}
          min={0}
          onCommit={(v) => set({ bleed_mm: v })}
        />
      </div>
      <div className="pp-paint-row is-compact">
        <PaintField
          paint={background}
          label="배경색"
          palette={palette}
          onChange={(p) => set({ background: p })}
        />
        <span className="t-body-m pp-paint-name">{background ? paintLabel(background) : "없음"}</span>
        <span className="t-body-m c-tertiary">배경</span>
      </div>
    </PropSection>
  );
}

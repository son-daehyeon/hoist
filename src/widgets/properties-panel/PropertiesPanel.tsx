import { useMemo } from "react";
import { useEd } from "@/entities/editor";
import type { Layer } from "@/shared/api";
import { PanelHeader } from "@/shared/ui";
import { ActionSection, AlignSection } from "./ActionSections";
import { ArtboardSection } from "./ArtboardSection";
import { EffectSection } from "./EffectSection";
import { ImageSection, ShapeSection } from "./KindSections";
import { docColors } from "./model";
import { CommonFillSection, FillSection, StrokeSection } from "./PaintSections";
import { TextSection } from "./TextSection";
import { TransformSection } from "./TransformSection";
import "./properties-panel.css";

export function PropertiesPanel() {
  const { doc, project, selection } = useEd();
  const palette = useMemo(() => (doc ? docColors(doc) : []), [doc]);
  if (!doc) return <div className="pp" />;

  const layers = selection.map((id) => doc.layers.find((l) => l.id === id)).filter((l): l is Layer => !!l);
  const one = layers.length === 1 ? layers[0] : null;
  const title = one
    ? one.name
    : layers.length > 1
      ? `레이어 ${layers.length}개`
      : (project?.name ?? "현수막");
  const paintable = one && one.kind.type !== "group" && one.kind.type !== "image";

  return (
    <div className="pp" data-popover-edge data-tour="props">
      <PanelHeader title={<span className="pp-title">{title}</span>} />
      <div className="pp-body" key={selection.join(",")}>
        {layers.length === 0 && <ArtboardSection doc={doc} palette={palette} />}
        {layers.length > 0 && <TransformSection layers={layers} doc={doc} />}
        {layers.length > 0 && <AlignSection />}
        {layers.length > 1 && <CommonFillSection layers={layers} palette={palette} />}
        {one && (
          <>
            <TextSection layer={one} />
            <ShapeSection layer={one} />
            <ImageSection layer={one} />
            {paintable && <FillSection layer={one} palette={palette} />}
            {one.kind.type !== "group" && <StrokeSection layer={one} palette={palette} />}
            <EffectSection layer={one} palette={palette} />
          </>
        )}
        {layers.length > 0 && <ActionSection />}
      </div>
    </div>
  );
}

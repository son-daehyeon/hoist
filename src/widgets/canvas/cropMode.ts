import { useEffect, useEffectEvent } from "react";
import { useEd } from "@/entities/editor";
import type { Pt } from "@/entities/path";
import type { Layer } from "@/shared/api";
import { isTextEntry, modalOpen } from "@/shared/lib";
import { cropMap, cropPx, dragEdges, edgesOf, type Edges } from "./crop";
import { centerOf, rotatePoint, type Handle } from "./geometry";
import { frameOf } from "./selection";

export function useCropMode(layers: Layer[], z: number, toWorld: (p: Pt) => Pt) {
  const { editing, setEditing, ops } = useEd();
  const cropping = editing?.kind === "crop" && !editing.pending ? editing : null;
  const cropLayer = cropping ? layers.find((l) => l.id === cropping.id) : undefined;
  const cropKind = cropLayer?.kind.type === "image" ? cropLayer.kind : null;
  const cropSpace = () => {
    if (!cropLayer || !cropKind) return null;
    const f = frameOf(layers, cropLayer);
    return { f, m: cropMap(cropKind, f.w, f.h) };
  };
  const pxDelta = (d: Pt) => {
    const c = cropSpace();
    if (!c) return { x: 0, y: 0 };
    const v = rotatePoint({ x: d.x / z, y: d.y / z }, { x: 0, y: 0 }, -c.f.deg);
    return { x: v.x / c.m.sx, y: v.y / c.m.sy };
  };
  const insideCrop = (p: Pt) => {
    const c = cropSpace();
    if (!c || !cropping) return false;
    const q = rotatePoint(toWorld(p), centerOf(c.f), -c.f.deg);
    const u = (q.x - c.f.x - c.m.ox) / c.m.sx;
    const v = (q.y - c.f.y - c.m.oy) / c.m.sy;
    const [l, t, r, b] = cropping.edges;
    return u >= l && u <= r && v >= t && v <= b;
  };
  const commitCrop = () => {
    if (!cropping || !cropKind) return setEditing(null);
    const next = cropPx(cropping.edges, cropKind.natural_w_px, cropKind.natural_h_px);
    const cur = cropPx(edgesOf(cropKind), cropKind.natural_w_px, cropKind.natural_h_px);
    if (JSON.stringify(next) === JSON.stringify(cur)) return setEditing(null);
    setEditing({ ...cropping, pending: true });
    void ops([{ op: "update_layer", id: cropping.id, patch: { kind: { crop_px: next } } }]).then(
      (ok) => ok || setEditing(null),
    );
  };
  const onCropKey = useEffectEvent((e: KeyboardEvent) => {
    if (!cropping || isTextEntry(e.target) || modalOpen()) return;
    if (e.key === "Enter") {
      e.preventDefault();
      commitCrop();
    } else if (e.key === "Escape") {
      e.preventDefault();
      setEditing(null);
    }
  });
  useEffect(() => {
    window.addEventListener("keydown", onCropKey);
    return () => window.removeEventListener("keydown", onCropKey);
  }, []);

  const dragTo = (drag: { handle: Handle | "move"; start: Pt; edges0: Edges }, p: Pt) => {
    if (!cropping || !cropKind) return;
    const d = pxDelta({ x: p.x - drag.start.x, y: p.y - drag.start.y });
    const { natural_w_px: nw, natural_h_px: nh } = cropKind;
    return setEditing({ ...cropping, edges: dragEdges(drag.edges0, drag.handle, d.x, d.y, nw, nh) });
  };

  return { cropping, contains: insideCrop, commit: commitCrop, dragTo };
}

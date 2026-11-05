import { useSyncExternalStore } from "react";
import type { EditorState } from "@/entities/editor";
import { api, errorMessage, type Layer, type LayerId, type Op } from "@/shared/api";
import { childrenOf, nextLayerId } from "@/shared/lib";
import { toast } from "@/shared/ui";
import { reorder, type Arrange } from "./arrange";
import { moved, unplaced, type Params } from "./drag";
import { hasAncestor, movable, paramsOf, parentMatrix } from "./selection";

const PASTE_STEP_MM = 10;

type Clip = {
  projectId: string;
  assetsDir: string;
  layers: Layer[];
  world: Record<LayerId, Params>;
  pastes: number;
};

let clip: Clip | null = null;
let assetsDir = "";

export const rememberAssetsDir = (dir: string) => {
  assetsDir = dir;
};
const listeners = new Set<() => void>();
export const hasClip = () => clip !== null;
const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => listeners.delete(fn);
};
export const useHasClip = () => useSyncExternalStore(subscribe, hasClip);

const round2 = (v: number) => Math.round(v * 100) / 100;
const joinPath = (dir: string, name: string) =>
  /[\\/]$/.test(dir) || !dir ? `${dir}${name}` : `${dir}/${name}`;

export function copyLayers({ doc, selection, projectId }: EditorState): boolean {
  const layers = doc?.layers ?? [];
  const picked = new Set(selection);
  const roots = layers.filter((l) => picked.has(l.id) && !hasAncestor(layers, l, picked));
  if (!roots.length) return false;
  const subtree = (l: Layer): Layer[] => [l, ...childrenOf(layers, l.id).flatMap(subtree)];
  clip = {
    projectId,
    assetsDir,
    layers: structuredClone(roots.flatMap(subtree)),
    world: Object.fromEntries(roots.map((l) => [l.id, paramsOf(layers, l)])),
    pastes: 0,
  };
  listeners.forEach((fn) => fn());
  return true;
}

export async function pasteLayers({ doc, projectId, ops, setSelection }: EditorState) {
  if (!clip || !doc) return;
  const from = clip;
  const layers = doc.layers;
  const shift = (from.pastes + 1) * PASTE_STEP_MM;
  const same = from.projectId === projectId;
  const assets = new Map<string, string>();
  try {
    for (const l of from.layers) {
      if (same || l.kind.type !== "image" || assets.has(l.kind.asset)) continue;
      const img = await api.importImage(projectId, joinPath(from.assetsDir, l.kind.asset));
      assets.set(l.kind.asset, img.asset);
    }
  } catch (e) {
    toast(errorMessage(e), "error");
    return;
  }
  let n = Number(nextLayerId(layers).slice(1));
  const ids = new Map(from.layers.map((l) => [l.id, `L${n++}`]));
  const tops: LayerId[] = [];
  const added = from.layers.map((l): Layer => {
    const copy: Layer = { ...structuredClone(l), id: ids.get(l.id)! };
    if (copy.kind.type === "image") copy.kind.asset = assets.get(copy.kind.asset) ?? copy.kind.asset;
    const world = from.world[l.id];
    if (!world) return { ...copy, parent: ids.get(l.parent ?? "") ?? null };
    const parent = same && l.parent && layers.some((x) => x.id === l.parent) ? l.parent : null;
    const local = unplaced(parentMatrix(layers, { parent }), moved(world, shift, shift));
    tops.push(copy.id);
    return { ...copy, parent, x_mm: round2(local.x), y_mm: round2(local.y), rotation_deg: round2(local.deg) };
  });
  from.pastes += 1;
  const ok = await ops(added.map((layer): Op => ({ op: "add_layer", layer })));
  if (ok) setSelection(tops);
}

export function arrangeOps(layers: Layer[], ids: LayerId[], dir: Arrange): Op[] {
  const roots = movable(layers, ids);
  const picked = new Set(roots.map((l) => l.id));
  const parents = [...new Set(roots.map((l) => l.parent ?? null))];
  return parents.flatMap((parent) =>
    reorder(
      childrenOf(layers, parent).map((l) => l.id),
      picked,
      dir,
    ).moves.map((m): Op => ({ op: "move_layer", id: m.id, parent, index: m.index })),
  );
}

export function arrange({ doc, selection, ops, locked }: EditorState, dir: Arrange) {
  const list = locked || !doc ? [] : arrangeOps(doc.layers, selection, dir);
  if (list.length) void ops(list);
}

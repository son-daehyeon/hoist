import type { useEditorState } from "@/entities/editor";
import { api, errorMessage, type ApplyResult, type Layer, type Op } from "@/shared/api";
import { childrenOf, fileName, nextLayerId, parentOffset } from "@/shared/lib";
import { toast, type MenuItem } from "@/shared/ui";
import { aabb, alignMoves, distributeMoves, union, type AlignMode, type Move } from "./align";

type Ed = ReturnType<typeof useEditorState>;
type BooleanKind = Parameters<typeof api.pathBoolean>[2];

export const BOOLEAN_OPS: { op: BooleanKind; label: string; hint: string }[] = [
  { op: "union", label: "합치기", hint: "고른 도형을 하나로 합쳐요" },
  { op: "subtract", label: "빼기", hint: "처음 고른 도형에서 나머지 모양을 빼요" },
  { op: "intersect", label: "교차", hint: "모두 겹친 부분만 남겨요" },
  { op: "exclude", label: "제외", hint: "겹친 부분만 뚫어요" },
];

export const IMAGE_FILE = /\.(png|jpe?g|webp|bmp|gif|tiff?)$/i;
const COMBINABLE = new Set(["path", "shape", "text"]);

export const ALIGN_MODES: { mode: AlignMode; label: string; menu: string }[] = [
  { mode: "left", label: "왼쪽", menu: "왼쪽 맞춤" },
  { mode: "center", label: "가운데", menu: "가운데 맞춤" },
  { mode: "right", label: "오른쪽", menu: "오른쪽 맞춤" },
  { mode: "top", label: "위", menu: "위 맞춤" },
  { mode: "middle", label: "중간", menu: "중간 맞춤" },
  { mode: "bottom", label: "아래", menu: "아래 맞춤" },
];

export const DISTRIBUTE_AXES: { axis: "x" | "y"; label: string }[] = [
  { axis: "x", label: "가로 간격 같게" },
  { axis: "y", label: "세로 간격 같게" },
];

const round2 = (v: number) => Math.round(v * 100) / 100;

export function objectActions(ed: Ed) {
  const { doc, selection, projectId, ops, applyResult, setSelection, locked } = ed;
  const all = doc?.layers ?? [];
  const picked = selection.map((id) => all.find((l) => l.id === id)).filter((l): l is Layer => !!l);
  const isPicked = (id: string | null | undefined): boolean => {
    const l = all.find((x) => x.id === id);
    return !!l && (selection.includes(l.id) || isPicked(l.parent));
  };
  const roots = picked.filter((l) => !isPicked(l.parent));

  const fromResult = async (task: Promise<ApplyResult>, done: string) => {
    try {
      const r = await task;
      const before = new Set(all.map((l) => l.id));
      const added = r.document.layers.filter((l) => !before.has(l.id)).map((l) => l.id);
      applyResult(r);
      if (added.length) setSelection(added);
      toast(done);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const duplicate = (at?: Record<string, { x: number; y: number }>) => {
    let n = Number(nextLayerId(all).slice(1));
    const added: Layer[] = [];
    const tops: string[] = [];
    const copy = (l: Layer, parent: string | null, top: boolean) => {
      const id = `L${n++}`;
      added.push({
        ...structuredClone(l),
        id,
        parent,
        name: top ? `${l.name} 사본` : l.name,
        x_mm: top ? (at?.[l.id]?.x ?? l.x_mm + 10) : l.x_mm,
        y_mm: top ? (at?.[l.id]?.y ?? l.y_mm + 10) : l.y_mm,
      });
      if (top) tops.push(id);
      childrenOf(all, l.id).forEach((c) => copy(c, id, false));
    };
    roots.forEach((l) => copy(l, l.parent ?? null, true));
    return ops(added.map((layer): Op => ({ op: "add_layer", layer }))).then((ok) => {
      if (ok) setSelection(tops);
      return ok;
    });
  };

  const importImages = async (paths: string[]) => {
    if (!doc || locked) return;
    const files = paths.filter((p) => IMAGE_FILE.test(p));
    if (files.length === 0) return toast("그림 파일(PNG·JPG)만 넣을 수 있어요", "error");
    try {
      const images = await Promise.all(files.map((p) => api.importImage(projectId, p)));
      const { width_mm: W, height_mm: H } = doc.artboard;
      const group = all.find((l) => !l.parent && l.kind.type === "group" && l.name === "그래픽");
      const origin = group
        ? { x: parentOffset(all, group).x + group.x_mm, y: parentOffset(all, group).y + group.y_mm }
        : { x: 0, y: 0 };
      let n = Number(nextLayerId(all).slice(1));
      const layers = images.map((img, i): Layer => {
        const ratio = img.width_px / img.height_px;
        const w = Math.min(W / 3, H * ratio);
        const h = w / ratio;
        return {
          id: `L${n++}`,
          name: fileName(files[i]).replace(/\.[^.]+$/, ""),
          parent: group?.id ?? null,
          kind: {
            type: "image",
            asset: img.asset,
            natural_w_px: img.width_px,
            natural_h_px: img.height_px,
            fit: "contain",
            crop_px: null,
            adjust: { brightness: 0, contrast: 0, saturation: 0 },
          },
          x_mm: (W - w) / 2 - origin.x + i * 10,
          y_mm: (H - h) / 2 - origin.y + i * 10,
          w_mm: w,
          h_mm: h,
          rotation_deg: 0,
          opacity: 1,
          visible: true,
          locked: false,
          fills: [],
          strokes: [],
          effects: [],
          strategy: "user_image",
          strategy_reason: "사용자가 직접 넣은 그림",
          pinned: [],
          source: null,
        };
      });
      const ok = await ops(layers.map((layer): Op => ({ op: "add_layer", layer })));
      if (ok) setSelection(layers.map((l) => l.id));
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const movable = roots.filter((l) => !l.locked);
  const items = movable.map((l) => {
    const o = parentOffset(all, l);
    return {
      id: l.id,
      box: aabb({ x: o.x + l.x_mm, y: o.y + l.y_mm, w: l.w_mm, h: l.h_mm }, l.rotation_deg),
    };
  });
  const move = (moves: Move[]) => {
    const list = moves.flatMap((m): Op[] => {
      const l = movable.find((x) => x.id === m.id);
      const patch = {
        ...(Math.abs(m.dx) > 0.005 && l ? { x_mm: round2(l.x_mm + m.dx) } : {}),
        ...(Math.abs(m.dy) > 0.005 && l ? { y_mm: round2(l.y_mm + m.dy) } : {}),
      };
      return l && Object.keys(patch).length ? [{ op: "update_layer", id: l.id, patch }] : [];
    });
    if (list.length) void ops(list);
  };
  const artboard = { x: 0, y: 0, w: doc?.artboard.width_mm ?? 0, h: doc?.artboard.height_mm ?? 0 };

  const canCombine = !locked && picked.length >= 2 && picked.every((l) => COMBINABLE.has(l.kind.type));
  const canOutline = !locked && picked.length === 1 && picked[0].kind.type === "text";
  const canEdit = !locked && roots.length > 0;

  return {
    canCombine,
    combine: (op: BooleanKind) =>
      fromResult(
        api.pathBoolean(
          projectId,
          picked.map((l) => l.id),
          op,
        ),
        `${BOOLEAN_OPS.find((b) => b.op === op)?.label ?? ""} 했어요`,
      ),
    canOutline,
    outline: () => fromResult(api.outlineText(projectId, picked[0].id), "글자를 윤곽선으로 바꿨어요"),
    canEdit,
    duplicate: () => void duplicate(),
    duplicateAt: duplicate,
    remove: () => void ops(roots.map((l): Op => ({ op: "delete_layer", id: l.id }))),
    canAlign: !locked && movable.length > 0,
    alignTarget: movable.length === 1 ? "현수막" : "고른 것끼리",
    align: (mode: AlignMode) =>
      move(alignMoves(items, items.length === 1 ? artboard : union(items.map((i) => i.box)), mode)),
    canDistribute: !locked && movable.length >= 3,
    distribute: (axis: "x" | "y") => move(distributeMoves(items, axis)),
    canImport: !locked && doc !== null,
    importImages,
    pickImage: async () => {
      const path = await api.pickImageFile();
      if (path) await importImages([path]);
    },
  };
}

export type ObjectActions = ReturnType<typeof objectActions>;

export function objectMenu(a: ObjectActions): MenuItem[] {
  return [
    { label: "복제", icon: "copy", shortcut: "Ctrl+D", disabled: !a.canEdit, onSelect: a.duplicate },
    { label: "삭제", icon: "trash", shortcut: "Delete", disabled: !a.canEdit, onSelect: a.remove },
    { label: "윤곽선 만들기", icon: "pen", disabled: !a.canOutline, onSelect: () => void a.outline() },
    ...BOOLEAN_OPS.map((b): MenuItem => ({
      label: `패스 ${b.label}`,
      disabled: !a.canCombine,
      onSelect: () => void a.combine(b.op),
    })),
  ];
}

export function alignMenu(a: ObjectActions): MenuItem[] {
  return [
    ...ALIGN_MODES.map((m): MenuItem => ({
      label: m.menu,
      disabled: !a.canAlign,
      onSelect: () => a.align(m.mode),
    })),
    ...DISTRIBUTE_AXES.map((d): MenuItem => ({
      label: d.label,
      disabled: !a.canDistribute,
      onSelect: () => a.distribute(d.axis),
    })),
  ];
}

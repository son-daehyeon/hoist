import type { useEditorState } from "@/entities/editor";
import { api, errorMessage, type ApplyResult, type Layer, type Op } from "@/shared/api";
import { childrenOf, fileName, nextLayerId, parentOffset } from "@/shared/lib";
import { toast, type MenuItem } from "@/shared/ui";

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

export function objectActions(ed: Ed) {
  const { doc, selection, projectId, ops, refresh, setSelection, locked } = ed;
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
      refresh();
      if (added.length) setSelection(added);
      toast(done);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const duplicate = () => {
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
        x_mm: top ? l.x_mm + 10 : l.x_mm,
        y_mm: top ? l.y_mm + 10 : l.y_mm,
      });
      if (top) tops.push(id);
      childrenOf(all, l.id).forEach((c) => copy(c, id, false));
    };
    roots.forEach((l) => copy(l, l.parent ?? null, true));
    void ops(added.map((layer): Op => ({ op: "add_layer", layer }))).then((ok) => ok && setSelection(tops));
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
          strategy: "generated_raster",
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
    duplicate,
    remove: () => void ops(roots.map((l): Op => ({ op: "delete_layer", id: l.id }))),
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

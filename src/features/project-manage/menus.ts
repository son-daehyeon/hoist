import { api, type Folder, type ProjectSummary } from "@/shared/api";
import type { MenuItem } from "@/shared/ui";
import type { ManageModal } from "./ManageModals";

export function projectMenu(
  p: ProjectSummary,
  inTrash: boolean,
  {
    run,
    ask,
    onGone,
  }: {
    run: (fn: () => Promise<unknown>, done?: string) => Promise<void>;
    ask: (modal: ManageModal) => void;
    onGone: (ids: string[]) => void;
  },
): MenuItem[] {
  return inTrash
    ? [
        {
          label: "복원",
          icon: "undo",
          onSelect: () => run(() => api.restoreProject(p.id), "복원했어요"),
        },
        "divider",
        {
          label: "완전히 삭제",
          icon: "trash",
          danger: true,
          onSelect: () => ask({ kind: "purge", project: p }),
        },
      ]
    : [
        { label: "이름 바꾸기", icon: "pen", onSelect: () => ask({ kind: "rename", project: p }) },
        {
          label: "복제",
          icon: "copy",
          onSelect: () =>
            run(async () => {
              const copy = await api.duplicateProject(p.id, `${p.name} 사본`);
              if (p.folder) await api.moveProject(copy.id, p.folder);
            }, "복제했어요"),
        },
        {
          label: "폴더로 옮기기",
          icon: "folder",
          onSelect: () => ask({ kind: "move", project: p }),
        },
        {
          label: "탐색기에서 보기",
          icon: "folder-open",
          onSelect: () => run(() => api.showInFolder(p.id)),
        },
        "divider",
        {
          label: "삭제",
          icon: "trash",
          danger: true,
          onSelect: () =>
            run(async () => {
              await api.trashProject(p.id);
              onGone([p.id]);
            }, "휴지통으로 옮겼어요"),
        },
      ];
}

export function folderMenu(f: Folder, ask: (modal: ManageModal) => void): MenuItem[] {
  return [
    { label: "이름 바꾸기", icon: "pen", onSelect: () => ask({ kind: "rename-folder", folder: f }) },
    "divider",
    {
      label: "삭제",
      icon: "trash",
      danger: true,
      onSelect: () => ask({ kind: "delete-folder", folder: f }),
    },
  ];
}

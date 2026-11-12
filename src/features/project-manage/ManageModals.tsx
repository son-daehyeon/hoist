import { api, type Folder, type ProjectSummary } from "@/shared/api";
import { ConfirmDialog, MoveDialog, NameDialog } from "./ProjectDialogs";

export type ManageModal =
  | { kind: "new-folder" | "empty-trash" }
  | { kind: "rename" | "move" | "purge"; project: ProjectSummary }
  | { kind: "rename-folder" | "delete-folder"; folder: Folder };

export function ManageModals({
  modal,
  onClose,
  run,
  projects,
  folders,
  currentFolder,
  trashCount,
  onGone,
}: {
  modal: ManageModal;
  onClose: () => void;
  run: (fn: () => Promise<unknown>, done?: string) => Promise<void>;
  projects: ProjectSummary[] | null;
  folders: Folder[];
  currentFolder: string | null;
  trashCount: number;
  onGone: (ids: string[]) => void;
}) {
  return (
    <>
      {modal.kind === "rename" && (
        <NameDialog
          title="이름 바꾸기"
          initial={modal.project.name}
          onClose={onClose}
          onSave={(name) => {
            onClose();
            void run(() => api.renameProject(modal.project.id, name));
          }}
        />
      )}
      {modal.kind === "new-folder" && (
        <NameDialog
          title="새 폴더"
          initial=""
          confirm="만들기"
          onClose={onClose}
          onSave={(name) => {
            onClose();
            void run(() => api.createFolder(name, currentFolder));
          }}
        />
      )}
      {modal.kind === "rename-folder" && (
        <NameDialog
          title="폴더 이름 바꾸기"
          initial={modal.folder.name}
          onClose={onClose}
          onSave={(name) => {
            onClose();
            void run(() => api.renameFolder(modal.folder.id, name));
          }}
        />
      )}
      {modal.kind === "move" && (
        <MoveDialog
          folders={folders}
          current={modal.project.folder ?? null}
          onClose={onClose}
          onMove={(folder) => {
            onClose();
            void run(() => api.moveProject(modal.project.id, folder), "옮겼어요");
          }}
        />
      )}
      {modal.kind === "delete-folder" && (
        <ConfirmDialog
          title="폴더 삭제"
          body={`‘${modal.folder.name}’ 폴더를 삭제할까요?`}
          note="안에 있는 프로젝트는 휴지통으로 가요."
          confirm="삭제"
          onClose={onClose}
          onConfirm={() => {
            const inside = (projects ?? []).filter((p) => !p.deleted_at).map((p) => p.id);
            onClose();
            void run(async () => {
              await api.deleteFolder(modal.folder.id);
              const left = new Set((await api.listProjects()).filter((p) => !p.deleted_at).map((p) => p.id));
              onGone(inside.filter((id) => !left.has(id)));
            }, "폴더를 삭제했어요");
          }}
        />
      )}
      {modal.kind === "purge" && (
        <ConfirmDialog
          title="완전히 삭제"
          body={`‘${modal.project.name}’을(를) 완전히 삭제할까요?`}
          note="삭제하면 되돌릴 수 없어요."
          confirm="완전히 삭제"
          onClose={onClose}
          onConfirm={() => {
            onClose();
            void run(() => api.deleteProject(modal.project.id), "삭제했어요");
          }}
        />
      )}
      {modal.kind === "empty-trash" && (
        <ConfirmDialog
          title="휴지통 비우기"
          body={`휴지통의 프로젝트 ${trashCount}개를 완전히 삭제할까요?`}
          note="삭제하면 되돌릴 수 없어요."
          confirm="비우기"
          onClose={onClose}
          onConfirm={() => {
            onClose();
            void run(() => api.emptyTrash(), "휴지통을 비웠어요");
          }}
        />
      )}
    </>
  );
}

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { AUTO_FIRST_MESSAGE, useAgentSessions } from "@/entities/editor";
import { api, errorMessage, type Folder, type ProjectSummary } from "@/shared/api";
import { ConfirmDialog, MoveDialog, NameDialog } from "@/features/project-manage";
import { NewProjectDialog } from "@/features/project-create";
import { dateLabel, pngSrc, sizeLabel } from "@/shared/lib";
import { pickProjectFolder } from "@/features/project-open";
import { AppMenu } from "@/widgets/app-menu";
import { SettingsDialog } from "@/features/settings";
import { TitleBar } from "@/widgets/title-bar";
import {
  Button,
  EmptyState,
  FolderTile,
  Icon,
  Logo,
  Menu,
  NavItem,
  ProjectTile,
  type MenuItem,
  type TileStatus,
  SearchField,
  ToolbarDropdown,
  toast,
} from "@/shared/ui";
import "./home.css";

type Nav = "recent" | "projects" | "trash";
type Modal =
  | { kind: "new" | "settings" | "new-folder" | "empty-trash" }
  | { kind: "rename" | "move" | "purge"; project: ProjectSummary }
  | { kind: "rename-folder" | "delete-folder"; folder: Folder }
  | null;

const NAV_LABEL: Record<Nav, string> = { recent: "최근", projects: "프로젝트", trash: "휴지통" };
const RECENT_MS = 30 * 86_400_000;
const DONE_BADGE_MS = 8000;
const byName = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name, "ko");

export function Home({
  tabs,
  startWithNew,
  onOpen,
  onGone,
}: {
  tabs: ReactNode;
  startWithNew: boolean;
  onOpen: (id: string, firstMessage?: string, name?: string) => void;
  onGone: (ids: string[]) => void;
}) {
  const [projects, setProjects] = useState<ProjectSummary[] | null>(null);
  const [folders, setFolders] = useState<Folder[]>([]);
  const sessions = useAgentSessions();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const ends = [...sessions.values()].flatMap((s) => (s.doneAt ? [s.doneAt + DONE_BADGE_MS] : []));
    const next = Math.min(...ends.filter((t) => t > now));
    if (!Number.isFinite(next)) return;
    const timer = window.setTimeout(() => setNow(Date.now()), next - Date.now());
    return () => window.clearTimeout(timer);
  }, [sessions, now]);
  const statusOf = (id: string): TileStatus | undefined => {
    const s = sessions.get(id);
    if (s?.busy) return { kind: "working", label: s.lastLabel };
    if (s?.doneAt && s.doneAt + DONE_BADGE_MS > now) return { kind: "done" };
  };
  const [loadedAt, setLoadedAt] = useState(0);
  const [nav, setNav] = useState<Nav>("projects");
  const [cwd, setCwd] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<"updated" | "name">("name");
  const [sortMenu, setSortMenu] = useState<DOMRect | null>(null);
  const [menu, setMenu] = useState<{ anchor: DOMRect; items: MenuItem[] } | null>(null);
  const [modal, setModal] = useState<Modal>(startWithNew ? { kind: "new" } : null);

  const reload = useCallback(() => {
    Promise.all([api.listProjects(), api.listFolders()])
      .then(([list, dirs]) => {
        setLoadedAt(Date.now());
        setProjects(list);
        setFolders(dirs);
      })
      .catch((e) => {
        setProjects([]);
        toast(errorMessage(e), "error");
      });
  }, []);
  useEffect(reload, [reload]);

  const run = async (fn: () => Promise<unknown>, done?: string) => {
    try {
      await fn();
      if (done) toast(done);
      reload();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const openFolder = useCallback(async () => {
    const id = await pickProjectFolder();
    if (id) onOpen(id);
  }, [onOpen]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!e.ctrlKey || modal) return;
      if (e.key.toLowerCase() === "n") {
        e.preventDefault();
        setModal({ kind: "new" });
      } else if (e.key.toLowerCase() === "o") {
        e.preventDefault();
        void openFolder();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [modal, openFolder]);

  // the folder being shown may have been deleted meanwhile
  const here = cwd && folders.some((f) => f.id === cwd) ? cwd : null;
  const trail = useMemo(() => {
    const path: Folder[] = [];
    for (let f = folders.find((x) => x.id === here); f; f = folders.find((x) => x.id === f?.parent)) {
      if (path.includes(f)) break;
      path.unshift(f);
    }
    return path;
  }, [folders, here]);

  const q = query.trim().toLowerCase();
  const shown = useMemo(() => {
    const all = projects ?? [];
    const matches = (p: ProjectSummary) => !q || p.name.toLowerCase().includes(q);
    const recent = (a: ProjectSummary, b: ProjectSummary) => b.updated_at.localeCompare(a.updated_at);
    if (nav === "trash")
      return all
        .filter((p) => p.deleted_at && matches(p))
        .sort((a, b) => (b.deleted_at ?? "").localeCompare(a.deleted_at ?? ""));
    const live = all.filter((p) => !p.deleted_at && matches(p));
    if (nav === "recent")
      return live.filter((p) => loadedAt - new Date(p.updated_at).getTime() < RECENT_MS).sort(recent);
    // a search looks through every folder; otherwise only the open one
    return live.filter((p) => q || (p.folder ?? null) === here).sort(sort === "name" ? byName : recent);
  }, [projects, loadedAt, nav, q, sort, here]);
  const shownFolders = useMemo(
    () => (nav === "projects" && !q ? folders.filter((f) => (f.parent ?? null) === here).sort(byName) : []),
    [folders, nav, q, here],
  );
  const countIn = (folder: string) =>
    (projects ?? []).filter((p) => !p.deleted_at && p.folder === folder).length;
  const trashCount = (projects ?? []).filter((p) => p.deleted_at).length;

  const projectMenu = (p: ProjectSummary) => (anchor: DOMRect) =>
    setMenu({
      anchor,
      items:
        nav === "trash"
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
                onSelect: () => setModal({ kind: "purge", project: p }),
              },
            ]
          : [
              { label: "이름 바꾸기", icon: "pen", onSelect: () => setModal({ kind: "rename", project: p }) },
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
                onSelect: () => setModal({ kind: "move", project: p }),
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
            ],
    });
  const folderMenu = (f: Folder) => (anchor: DOMRect) =>
    setMenu({
      anchor,
      items: [
        { label: "이름 바꾸기", icon: "pen", onSelect: () => setModal({ kind: "rename-folder", folder: f }) },
        "divider",
        {
          label: "삭제",
          icon: "trash",
          danger: true,
          onSelect: () => setModal({ kind: "delete-folder", folder: f }),
        },
      ],
    });

  const go = (next: Nav) => {
    setNav(next);
    setCwd(null);
    setQuery("");
  };
  const newButton = (
    <Button kind="primary" size="l" icon="plus" onClick={() => setModal({ kind: "new" })}>
      새 프로젝트
    </Button>
  );
  const empty = projects !== null && shown.length === 0 && shownFolders.length === 0;

  return (
    <div className="app">
      <TitleBar
        left={
          <AppMenu
            onNew={() => setModal({ kind: "new" })}
            onOpenFolder={() => void openFolder()}
            onSettings={() => setModal({ kind: "settings" })}
          />
        }
        title={
          <>
            <Logo size={20} />
            {tabs}
          </>
        }
      />
      <div className="home-body">
        <nav className="home-nav">
          <NavItem icon="clock" label="최근" active={nav === "recent"} onClick={() => go("recent")} />
          <NavItem
            icon="folder"
            label="프로젝트"
            active={nav === "projects"}
            onClick={() => go("projects")}
          />
          <NavItem icon="trash" label="휴지통" active={nav === "trash"} onClick={() => go("trash")} />
          <div className="home-nav-bottom">
            <NavItem icon="settings" label="설정" onClick={() => setModal({ kind: "settings" })} />
          </div>
        </nav>
        <main className="home-main">
          <div className="home-header">
            {nav === "projects" && trail.length > 0 ? (
              <h1 className="home-trail t-title-l">
                <button type="button" className="home-crumb c-tertiary" onClick={() => setCwd(null)}>
                  프로젝트
                </button>
                {trail.map((f, i) => (
                  <span key={f.id} className="home-crumb-step">
                    <Icon name="chevron-right" size={16} />
                    {i === trail.length - 1 ? (
                      <span className="bs-ellipsis">{f.name}</span>
                    ) : (
                      <button type="button" className="home-crumb c-tertiary" onClick={() => setCwd(f.id)}>
                        {f.name}
                      </button>
                    )}
                  </span>
                ))}
              </h1>
            ) : (
              <h1 className="t-title-l">{NAV_LABEL[nav]}</h1>
            )}
            <span className="home-spacer" />
            <SearchField
              placeholder="프로젝트 검색"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <span className="home-gap" />
            {nav === "projects" && (
              <Button size="l" icon="folder" onClick={() => setModal({ kind: "new-folder" })}>
                새 폴더
              </Button>
            )}
            {nav === "trash" ? (
              <Button
                size="l"
                icon="trash"
                disabled={trashCount === 0}
                onClick={() => setModal({ kind: "empty-trash" })}
              >
                휴지통 비우기
              </Button>
            ) : (
              newButton
            )}
          </div>
          <div className="home-sub">
            {nav === "projects" && (
              <ToolbarDropdown
                label={sort === "updated" ? "최근 수정 순" : "이름 순"}
                chevron={false}
                onOpen={setSortMenu}
              />
            )}
            <span className="t-caption c-tertiary">
              {shownFolders.length > 0 && `폴더 ${shownFolders.length}개 · `}프로젝트 {shown.length}개
              {nav === "trash" && trashCount > 0 && " · 비우기 전까지 복원할 수 있어요"}
            </span>
          </div>

          {empty ? (
            query ? (
              <EmptyState
                icon="search"
                title="찾는 프로젝트가 없어요"
                description="다른 이름으로 검색해 보세요."
              />
            ) : nav === "trash" ? (
              <EmptyState
                icon="trash"
                title="휴지통이 비어 있어요"
                description="삭제한 프로젝트는 여기로 와요. 비우기 전까지 복원할 수 있어요."
              />
            ) : nav === "projects" && here ? (
              <EmptyState
                icon="folder"
                title="빈 폴더예요"
                description="프로젝트의 더 보기 메뉴에서 ‘폴더로 옮기기’로 넣을 수 있어요."
              />
            ) : (
              <EmptyState
                icon="hoist"
                title="아직 만든 현수막이 없어요"
                description="받은 현수막 이미지를 올리면 레이어로 다시 만들어 드려요"
                action={newButton}
              />
            )
          ) : (
            <div className="home-grid">
              {shownFolders.map((f) => (
                <FolderTile
                  key={f.id}
                  title={f.name}
                  meta={`프로젝트 ${countIn(f.id)}개`}
                  onOpen={() => setCwd(f.id)}
                  onMore={folderMenu(f)}
                />
              ))}
              {shown.map((p) => (
                <ProjectTile
                  key={p.id}
                  title={p.name}
                  meta={`${sizeLabel(p.width_mm, p.height_mm)} · ${dateLabel(p.deleted_at ?? p.updated_at)}`}
                  thumbnail={pngSrc(p.thumbnail_png)}
                  status={statusOf(p.id)}
                  onOpen={nav === "trash" ? () => {} : () => onOpen(p.id, undefined, p.name)}
                  onMore={projectMenu(p)}
                />
              ))}
            </div>
          )}
        </main>
      </div>

      {sortMenu && (
        <Menu
          anchor={sortMenu}
          onClose={() => setSortMenu(null)}
          items={[
            { label: "이름 순", checked: sort === "name", onSelect: () => setSort("name") },
            { label: "최근 수정 순", checked: sort === "updated", onSelect: () => setSort("updated") },
          ]}
        />
      )}
      {menu && <Menu anchor={menu.anchor} align="end" onClose={() => setMenu(null)} items={menu.items} />}

      {modal?.kind === "new" && (
        <NewProjectDialog
          onClose={() => setModal(null)}
          onCreated={async (p, withOriginal) => {
            if (nav === "projects" && here) await api.moveProject(p.id, here).catch(() => {});
            onOpen(p.id, withOriginal ? AUTO_FIRST_MESSAGE : undefined, p.name);
          }}
        />
      )}
      {modal?.kind === "settings" && <SettingsDialog onClose={() => setModal(null)} />}
      {modal?.kind === "rename" && (
        <NameDialog
          title="이름 바꾸기"
          initial={modal.project.name}
          onClose={() => setModal(null)}
          onSave={(name) => {
            setModal(null);
            void run(() => api.renameProject(modal.project.id, name));
          }}
        />
      )}
      {modal?.kind === "new-folder" && (
        <NameDialog
          title="새 폴더"
          initial=""
          confirm="만들기"
          onClose={() => setModal(null)}
          onSave={(name) => {
            setModal(null);
            void run(() => api.createFolder(name, here));
          }}
        />
      )}
      {modal?.kind === "rename-folder" && (
        <NameDialog
          title="폴더 이름 바꾸기"
          initial={modal.folder.name}
          onClose={() => setModal(null)}
          onSave={(name) => {
            setModal(null);
            void run(() => api.renameFolder(modal.folder.id, name));
          }}
        />
      )}
      {modal?.kind === "move" && (
        <MoveDialog
          name={modal.project.name}
          folders={folders}
          current={modal.project.folder ?? null}
          onClose={() => setModal(null)}
          onMove={(folder) => {
            setModal(null);
            void run(() => api.moveProject(modal.project.id, folder), "옮겼어요");
          }}
        />
      )}
      {modal?.kind === "delete-folder" && (
        <ConfirmDialog
          title="폴더 삭제"
          body={`‘${modal.folder.name}’ 폴더를 삭제할까요?`}
          note="안에 있는 프로젝트는 휴지통으로 가요."
          confirm="삭제"
          onClose={() => setModal(null)}
          onConfirm={() => {
            const inside = (projects ?? []).filter((p) => !p.deleted_at).map((p) => p.id);
            setModal(null);
            void run(async () => {
              await api.deleteFolder(modal.folder.id);
              const left = new Set((await api.listProjects()).filter((p) => !p.deleted_at).map((p) => p.id));
              onGone(inside.filter((id) => !left.has(id)));
            }, "폴더를 삭제했어요");
          }}
        />
      )}
      {modal?.kind === "purge" && (
        <ConfirmDialog
          title="완전히 삭제"
          body={`‘${modal.project.name}’을(를) 완전히 삭제할까요?`}
          note="삭제하면 되돌릴 수 없어요."
          confirm="완전히 삭제"
          onClose={() => setModal(null)}
          onConfirm={() => {
            setModal(null);
            void run(() => api.deleteProject(modal.project.id), "삭제했어요");
          }}
        />
      )}
      {modal?.kind === "empty-trash" && (
        <ConfirmDialog
          title="휴지통 비우기"
          body={`휴지통의 프로젝트 ${trashCount}개를 완전히 삭제할까요?`}
          note="삭제하면 되돌릴 수 없어요."
          confirm="비우기"
          onClose={() => setModal(null)}
          onConfirm={() => {
            setModal(null);
            void run(() => api.emptyTrash(), "휴지통을 비웠어요");
          }}
        />
      )}
    </div>
  );
}

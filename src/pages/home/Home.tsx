import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { AUTO_FIRST_MESSAGE, useAgentSessions } from "@/entities/editor";
import { api } from "@/shared/api";
import { folderMenu, ManageModals, projectMenu, type ManageModal } from "@/features/project-manage";
import { NewProjectDialog } from "@/features/project-create";
import { dateLabel, modalOpen, pngSrc, sizeLabel } from "@/shared/lib";
import { pickProjectFolder } from "@/features/project-open";
import { AppMenu } from "@/widgets/app-menu";
import { SettingsDialog } from "@/features/settings";
import { TitleBar } from "@/widgets/title-bar";
import {
  Button,
  EmptyState,
  Icon,
  Logo,
  Menu,
  NavItem,
  type MenuItem,
  SearchField,
  ToolbarDropdown,
} from "@/shared/ui";
import { StartCard } from "@/features/onboarding";
import { folderTrail, NAV_LABEL, visibleFolders, visibleProjects, type Nav, type Sort } from "./libraryView";
import { FolderTile, ProjectTile, type TileStatus } from "./ProjectTile";
import { useLibrary } from "./useLibrary";
import "./home.css";

type Modal = { kind: "new" } | { kind: "settings" } | ManageModal | null;

const DONE_BADGE_MS = 8000;

function useTileStatus() {
  const sessions = useAgentSessions();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const ends = [...sessions.values()].flatMap((s) => (s.doneAt ? [s.doneAt + DONE_BADGE_MS] : []));
    const next = Math.min(...ends.filter((t) => t > now));
    if (!Number.isFinite(next)) return;
    const timer = window.setTimeout(() => setNow(Date.now()), next - Date.now());
    return () => window.clearTimeout(timer);
  }, [sessions, now]);
  return (id: string): TileStatus | undefined => {
    const s = sessions.get(id);
    if (s?.busy) return { kind: "working", label: s.lastLabel };
    if (s?.doneAt && s.doneAt + DONE_BADGE_MS > now) return { kind: "done" };
  };
}

export function Home({
  tabs,
  startWithNew,
  onOpen,
  onGone,
  onStartTutorial,
}: {
  tabs: ReactNode;
  startWithNew: boolean;
  onOpen: (id: string, firstMessage?: string, name?: string) => void;
  onGone: (ids: string[]) => void;
  onStartTutorial?: () => void;
}) {
  const statusOf = useTileStatus();
  const { projects, folders, loadedAt, run } = useLibrary();
  const [nav, setNav] = useState<Nav>("projects");
  const [folderId, setFolderId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("name");
  const [sortMenu, setSortMenu] = useState<DOMRect | null>(null);
  const [menu, setMenu] = useState<{ anchor: DOMRect; items: MenuItem[] } | null>(null);
  const [modal, setModal] = useState<Modal>(startWithNew ? { kind: "new" } : null);

  const openFolder = useCallback(async () => {
    const id = await pickProjectFolder();
    if (id) onOpen(id);
  }, [onOpen]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!e.ctrlKey || modalOpen()) return;
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
  }, [openFolder]);

  // the folder being shown may have been deleted meanwhile
  const currentFolder = folderId && folders.some((f) => f.id === folderId) ? folderId : null;
  const trail = useMemo(() => folderTrail(folders, currentFolder), [folders, currentFolder]);
  const search = query.trim().toLowerCase();
  const shownProjects = useMemo(
    () => visibleProjects({ projects, loadedAt, nav, search, sort, currentFolder }),
    [projects, loadedAt, nav, search, sort, currentFolder],
  );
  const shownFolders = useMemo(
    () => visibleFolders(folders, nav, search, currentFolder),
    [folders, nav, search, currentFolder],
  );
  const countIn = (folder: string) =>
    (projects ?? []).filter((p) => !p.deleted_at && p.folder === folder).length;
  const trashCount = (projects ?? []).filter((p) => p.deleted_at).length;

  const switchNav = (next: Nav) => {
    setNav(next);
    setFolderId(null);
    setQuery("");
  };
  const newButton = (
    <Button
      kind="primary"
      size="l"
      icon="plus"
      onClick={() => setModal({ kind: "new" })}
      data-tour="new-project"
    >
      새 프로젝트
    </Button>
  );
  const empty = projects !== null && shownProjects.length === 0 && shownFolders.length === 0;

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
          <NavItem icon="clock" label="최근" active={nav === "recent"} onClick={() => switchNav("recent")} />
          <NavItem
            icon="folder"
            label="프로젝트"
            active={nav === "projects"}
            onClick={() => switchNav("projects")}
          />
          <NavItem icon="trash" label="휴지통" active={nav === "trash"} onClick={() => switchNav("trash")} />
          <div className="home-nav-bottom">
            <NavItem icon="settings" label="설정" onClick={() => setModal({ kind: "settings" })} />
          </div>
        </nav>
        <main className="home-main">
          <div className="home-header">
            {nav === "projects" && trail.length > 0 ? (
              <h1 className="home-trail t-title-l">
                <button type="button" className="home-crumb c-tertiary" onClick={() => setFolderId(null)}>
                  프로젝트
                </button>
                {trail.map((f, i) => (
                  <span key={f.id} className="home-crumb-step">
                    <Icon name="chevron-right" size={16} />
                    {i === trail.length - 1 ? (
                      <span className="bs-ellipsis">{f.name}</span>
                    ) : (
                      <button
                        type="button"
                        className="home-crumb c-tertiary"
                        onClick={() => setFolderId(f.id)}
                      >
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
            {nav === "trash" && trashCount > 0 && (
              <span className="t-caption c-tertiary">비우기 전까지 복원할 수 있어요</span>
            )}
          </div>

          {onStartTutorial ? (
            <StartCard onStart={onStartTutorial} />
          ) : empty ? (
            query ? (
              <EmptyState icon="search" title="찾는 프로젝트가 없어요" />
            ) : nav === "trash" ? (
              <EmptyState icon="trash" title="휴지통이 비어 있어요" />
            ) : nav === "projects" && currentFolder ? (
              <EmptyState
                icon="folder"
                title="빈 폴더예요"
                description="프로젝트의 더 보기 메뉴에서 ‘폴더로 옮기기’로 넣을 수 있어요."
              />
            ) : (
              <EmptyState icon="hoist" title="아직 만든 현수막이 없어요" action={newButton} />
            )
          ) : (
            <div className="home-grid">
              {shownFolders.map((f) => (
                <FolderTile
                  key={f.id}
                  title={f.name}
                  meta={`프로젝트 ${countIn(f.id)}개`}
                  onOpen={() => setFolderId(f.id)}
                  onMore={(anchor) => setMenu({ anchor, items: folderMenu(f, setModal) })}
                />
              ))}
              {shownProjects.map((p) => (
                <ProjectTile
                  key={p.id}
                  title={p.name}
                  meta={`${sizeLabel(p.width_mm, p.height_mm)} · ${dateLabel(p.deleted_at ?? p.updated_at)}`}
                  thumbnail={pngSrc(p.thumbnail_png)}
                  status={statusOf(p.id)}
                  onOpen={nav === "trash" ? () => {} : () => onOpen(p.id, undefined, p.name)}
                  onMore={(anchor) =>
                    setMenu({
                      anchor,
                      items: projectMenu(p, nav === "trash", { run, ask: setModal, onGone }),
                    })
                  }
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
            if (nav === "projects" && currentFolder)
              await api.moveProject(p.id, currentFolder).catch(() => {});
            onOpen(p.id, withOriginal ? AUTO_FIRST_MESSAGE : undefined, p.name);
          }}
        />
      )}
      {modal?.kind === "settings" && <SettingsDialog onClose={() => setModal(null)} />}
      {modal && modal.kind !== "new" && modal.kind !== "settings" && (
        <ManageModals
          modal={modal}
          onClose={() => setModal(null)}
          run={run}
          projects={projects}
          folders={folders}
          currentFolder={currentFolder}
          trashCount={trashCount}
          onGone={onGone}
        />
      )}
    </div>
  );
}

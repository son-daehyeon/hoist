import { useCallback, useEffect, useMemo, useState } from "react";
import { api, errorMessage, type ProjectSummary } from "@/shared/api";
import { ConfirmDeleteDialog, RenameDialog } from "@/features/project-manage";
import { NewProjectDialog } from "@/features/project-create";
import { dateLabel, pngSrc, sizeLabel } from "@/shared/lib";
import { pickProjectFolder } from "@/features/project-open";
import { AppMenu } from "@/widgets/app-menu";
import { SettingsDialog } from "@/features/settings";
import { TitleBar } from "@/widgets/title-bar";
import {
  Button,
  EmptyState,
  Icon,
  IconButton,
  Menu,
  NavItem,
  ProjectListRow,
  ProjectTile,
  SearchField,
  ToolbarDropdown,
  toast,
} from "@/shared/ui";
import "./home.css";

type Nav = "recent" | "all" | "trash";
type Modal =
  { kind: "new" } | { kind: "settings" } | { kind: "rename" | "delete"; project: ProjectSummary } | null;

const NAV_LABEL: Record<Nav, string> = { recent: "최근 항목", all: "모든 프로젝트", trash: "휴지통" };
const RECENT_MS = 30 * 86_400_000;

export function Home({
  startWithNew,
  onOpen,
}: {
  startWithNew: boolean;
  onOpen: (id: string, firstMessage?: string) => void;
}) {
  const [projects, setProjects] = useState<ProjectSummary[] | null>(null);
  const [loadedAt, setLoadedAt] = useState(0);
  const [nav, setNav] = useState<Nav>("recent");
  const [view, setView] = useState<"grid" | "list">("grid");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<"updated" | "name">("updated");
  const [sortMenu, setSortMenu] = useState<DOMRect | null>(null);
  const [tileMenu, setTileMenu] = useState<{ anchor: DOMRect; project: ProjectSummary } | null>(null);
  const [modal, setModal] = useState<Modal>(startWithNew ? { kind: "new" } : null);

  const reload = useCallback(() => {
    api
      .listProjects()
      .then((list) => {
        setLoadedAt(Date.now());
        setProjects(list);
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

  const shown = useMemo(() => {
    if (!projects || nav === "trash") return [];
    const q = query.trim().toLowerCase();
    return projects
      .filter((p) => nav === "all" || loadedAt - new Date(p.updated_at).getTime() < RECENT_MS)
      .filter((p) => !q || p.name.toLowerCase().includes(q))
      .sort((a, b) =>
        sort === "name" ? a.name.localeCompare(b.name, "ko") : b.updated_at.localeCompare(a.updated_at),
      );
  }, [projects, loadedAt, nav, query, sort]);

  const more = (project: ProjectSummary) => (anchor: DOMRect) => setTileMenu({ anchor, project });
  const newButton = (
    <Button kind="primary" size="l" icon="plus" onClick={() => setModal({ kind: "new" })}>
      새 프로젝트
    </Button>
  );

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
            <span className="app-logo">
              <Icon name="layers" size={12} />
            </span>
            <span className="t-label-m">Banner Studio</span>
          </>
        }
      />
      <div className="home-body">
        <nav className="home-nav">
          <NavItem
            icon="clock"
            label="최근 항목"
            active={nav === "recent"}
            onClick={() => setNav("recent")}
          />
          <NavItem icon="folder" label="모든 프로젝트" active={nav === "all"} onClick={() => setNav("all")} />
          <NavItem icon="trash" label="휴지통" active={nav === "trash"} onClick={() => setNav("trash")} />
          <div className="home-nav-bottom">
            <NavItem icon="settings" label="설정" onClick={() => setModal({ kind: "settings" })} />
          </div>
        </nav>
        <main className="home-main">
          <div className="home-header">
            <h1 className="t-title-l">{NAV_LABEL[nav]}</h1>
            <span className="home-spacer" />
            <SearchField
              placeholder="프로젝트 검색"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <span className="titlebar-sep" />
            <IconButton
              icon="grid"
              label="격자로 보기"
              active={view === "grid"}
              onClick={() => setView("grid")}
            />
            <IconButton
              icon="list"
              label="목록으로 보기"
              active={view === "list"}
              onClick={() => setView("list")}
            />
            <span className="home-gap" />
            {newButton}
          </div>
          <div className="home-sub">
            <ToolbarDropdown
              label={sort === "updated" ? "최근 수정 순" : "이름 순"}
              chevron={false}
              onOpen={setSortMenu}
            />
            <span className="t-caption c-tertiary">프로젝트 {shown.length}개</span>
          </div>

          {projects && shown.length === 0 ? (
            nav === "trash" ? (
              <EmptyState
                icon="trash"
                title="휴지통이 비어 있어요"
                description="삭제한 프로젝트는 바로 지워져요."
              />
            ) : query ? (
              <EmptyState
                icon="search"
                title="찾는 프로젝트가 없어요"
                description="다른 이름으로 검색해 보세요."
              />
            ) : (
              <EmptyState
                title="아직 만든 현수막이 없어요"
                description="받은 현수막 이미지를 올리면 레이어로 다시 만들어 드려요"
                action={newButton}
              />
            )
          ) : view === "grid" ? (
            <div className="home-grid">
              {shown.map((p) => (
                <ProjectTile
                  key={p.id}
                  title={p.name}
                  meta={`${sizeLabel(p.width_mm, p.height_mm)} · ${dateLabel(p.updated_at)}`}
                  thumbnail={pngSrc(p.thumbnail_png)}
                  onOpen={() => onOpen(p.id)}
                  onMore={more(p)}
                />
              ))}
            </div>
          ) : (
            <div className="home-list">
              <div className="home-list-head t-micro c-tertiary">
                <span />
                <span>이름</span>
                <span>크기</span>
                <span>수정</span>
                <span />
              </div>
              {shown.map((p) => (
                <ProjectListRow
                  key={p.id}
                  title={p.name}
                  size={sizeLabel(p.width_mm, p.height_mm)}
                  date={dateLabel(p.updated_at)}
                  thumbnail={pngSrc(p.thumbnail_png)}
                  onOpen={() => onOpen(p.id)}
                  onMore={more(p)}
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
            { label: "최근 수정 순", checked: sort === "updated", onSelect: () => setSort("updated") },
            { label: "이름 순", checked: sort === "name", onSelect: () => setSort("name") },
          ]}
        />
      )}
      {tileMenu && (
        <Menu
          anchor={tileMenu.anchor}
          align="end"
          onClose={() => setTileMenu(null)}
          items={[
            {
              label: "이름 바꾸기",
              icon: "pen",
              onSelect: () => setModal({ kind: "rename", project: tileMenu.project }),
            },
            {
              label: "복제",
              icon: "copy",
              onSelect: () => run(() => api.duplicateProject(tileMenu.project.id), "복제했어요"),
            },
            {
              label: "폴더에서 보기",
              icon: "folder-open",
              onSelect: () => run(() => api.showInFolder(tileMenu.project.id)),
            },
            "divider",
            {
              label: "삭제",
              icon: "trash",
              danger: true,
              onSelect: () => setModal({ kind: "delete", project: tileMenu.project }),
            },
          ]}
        />
      )}

      {modal?.kind === "new" && (
        <NewProjectDialog
          onClose={() => setModal(null)}
          onCreated={(p, withOriginal) =>
            onOpen(p.id, withOriginal ? "원본과 똑같이 레이어로 만들어줘" : undefined)
          }
        />
      )}
      {modal?.kind === "settings" && <SettingsDialog onClose={() => setModal(null)} />}
      {modal?.kind === "rename" && (
        <RenameDialog
          initial={modal.project.name}
          onClose={() => setModal(null)}
          onSave={(name) => {
            setModal(null);
            void run(() => api.renameProject(modal.project.id, name));
          }}
        />
      )}
      {modal?.kind === "delete" && (
        <ConfirmDeleteDialog
          name={modal.project.name}
          onClose={() => setModal(null)}
          onConfirm={() => {
            setModal(null);
            void run(() => api.deleteProject(modal.project.id), "삭제했어요");
          }}
        />
      )}
    </div>
  );
}

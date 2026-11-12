import type { Folder, ProjectSummary } from "@/shared/api";
import { byName } from "@/shared/lib";

export type Nav = "recent" | "projects" | "trash";
export type Sort = "updated" | "name";

export const NAV_LABEL: Record<Nav, string> = { recent: "최근", projects: "프로젝트", trash: "휴지통" };
const RECENT_MS = 30 * 86_400_000;

export function folderTrail(folders: Folder[], currentFolder: string | null) {
  const path: Folder[] = [];
  for (let f = folders.find((x) => x.id === currentFolder); f; f = folders.find((x) => x.id === f?.parent)) {
    if (path.includes(f)) break;
    path.unshift(f);
  }
  return path;
}

export function visibleProjects({
  projects,
  loadedAt,
  nav,
  search,
  sort,
  currentFolder,
}: {
  projects: ProjectSummary[] | null;
  loadedAt: number;
  nav: Nav;
  search: string;
  sort: Sort;
  currentFolder: string | null;
}) {
  const all = projects ?? [];
  const matches = (p: ProjectSummary) => !search || p.name.toLowerCase().includes(search);
  const recent = (a: ProjectSummary, b: ProjectSummary) => b.updated_at.localeCompare(a.updated_at);
  if (nav === "trash")
    return all
      .filter((p) => p.deleted_at && matches(p))
      .sort((a, b) => (b.deleted_at ?? "").localeCompare(a.deleted_at ?? ""));
  const live = all.filter((p) => !p.deleted_at && matches(p));
  if (nav === "recent")
    return live.filter((p) => loadedAt - new Date(p.updated_at).getTime() < RECENT_MS).sort(recent);
  // a search looks through every folder; otherwise only the open one
  return live
    .filter((p) => search || (p.folder ?? null) === currentFolder)
    .sort(sort === "name" ? byName : recent);
}

export const visibleFolders = (folders: Folder[], nav: Nav, search: string, currentFolder: string | null) =>
  nav === "projects" && !search
    ? folders.filter((f) => (f.parent ?? null) === currentFolder).sort(byName)
    : [];

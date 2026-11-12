import type { Backend } from "../backend";
import type { Folder, ProjectSummary } from "../types";
import { renderComposite } from "./raster";
import { createMock, getProject, projects, seed, sleep, snapshot, uid } from "./store";

const folders: Folder[] = [];
const placement = new Map<string, string>();
const trash = new Map<string, string>();

export const library = {
  listProjects: async () => {
    seed();
    await sleep(60);
    return [...projects.values()].map(({ project: pr, doc }): ProjectSummary => ({
      id: pr.id,
      name: pr.name,
      width_mm: pr.width_mm,
      height_mm: pr.height_mm,
      updated_at: pr.updated_at,
      thumbnail_png: renderComposite(doc, 480),
      folder: placement.get(pr.id) ?? null,
      deleted_at: trash.get(pr.id) ?? null,
    }));
  },
  listFolders: async () => structuredClone(folders),
  createFolder: async (name, parent) => {
    const folder: Folder = { id: uid(), name: name.trim(), parent };
    folders.push(folder);
    return structuredClone(folder);
  },
  renameFolder: async (id, name) => {
    const folder = folders.find((f) => f.id === id);
    if (folder) folder.name = name.trim();
    return null;
  },
  deleteFolder: async (id) => {
    const gone = [id];
    for (let i = 0; i < gone.length; i++)
      gone.push(...folders.filter((f) => f.parent === gone[i] && !gone.includes(f.id)).map((f) => f.id));
    for (const [project, folder] of [...placement]) {
      if (!gone.includes(folder)) continue;
      placement.delete(project);
      trash.set(project, new Date().toISOString());
    }
    folders.splice(0, folders.length, ...folders.filter((f) => !gone.includes(f.id)));
    return null;
  },
  moveProject: async (id, folder) => {
    getProject(id);
    if (folder) placement.set(id, folder);
    else placement.delete(id);
    return null;
  },
  trashProject: async (id) => {
    getProject(id);
    trash.set(id, new Date().toISOString());
    return null;
  },
  restoreProject: async (id) => {
    trash.delete(id);
    return null;
  },
  emptyTrash: async () => {
    for (const id of trash.keys()) projects.delete(id);
    trash.clear();
    return null;
  },
  createProject: async ({ name, width_mm, height_mm, original_path, fit_mode }) => {
    seed();
    const p = createMock(name, width_mm, height_mm, Boolean(original_path));
    p.project.fit_mode = fit_mode ?? "ask";
    return structuredClone(p.project);
  },
  deleteProject: async (id) => {
    getProject(id);
    projects.delete(id);
    placement.delete(id);
    trash.delete(id);
    return null;
  },
  renameProject: async (id, name) => {
    const p = getProject(id);
    p.project.name = name;
    return structuredClone(p.project);
  },
  duplicateProject: async (id, name) => {
    const src = getProject(id);
    const p = createMock(name, src.project.width_mm, src.project.height_mm, Boolean(src.original));
    p.doc = structuredClone(src.doc);
    snapshot(p, "user", "복제");
    return structuredClone(p.project);
  },
  showInFolder: async (id) => {
    getProject(id);
    return null;
  },
} satisfies Partial<Backend>;

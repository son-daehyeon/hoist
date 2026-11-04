import { api } from "@/shared/api";
import { fileName } from "@/shared/lib";
import { toast } from "@/shared/ui";

export async function pickProjectFolder() {
  const dir = await api.pickFolder();
  if (!dir) return null;
  const id = fileName(dir.replace(/[\\/]+$/, ""));
  const all = await api.listProjects();
  if (all.some((p) => p.id === id)) return id;
  toast("Banner Studio 프로젝트 폴더가 아니에요", "error");
  return null;
}

import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, type Folder, type ProjectSummary } from "@/shared/api";
import { toast } from "@/shared/ui";

export function useLibrary() {
  const [projects, setProjects] = useState<ProjectSummary[] | null>(null);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [loadedAt, setLoadedAt] = useState(0);

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

  return { projects, folders, loadedAt, run };
}

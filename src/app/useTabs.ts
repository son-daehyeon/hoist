import { useCallback, useEffect, useState } from "react";
import type { ProjectTab } from "@/widgets/title-bar";

export function useTabs() {
  const [tabs, setTabs] = useState<ProjectTab[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [newProject, setNewProject] = useState(false);
  const [firstMessages, setFirstMessages] = useState<Record<string, string>>({});

  const open = useCallback((id: string, firstMessage?: string, name = "") => {
    if (firstMessage) setFirstMessages((m) => ({ ...m, [id]: firstMessage }));
    setTabs((t) => (t.some((x) => x.id === id) ? t : [...t, { id, name }]));
    setActive(id);
  }, []);
  const close = useCallback((ids: string[]) => {
    setTabs((t) => t.filter((x) => !ids.includes(x.id)));
    setActive((a) => (a && ids.includes(a) ? null : a));
  }, []);
  const renameTab = useCallback((id: string, name: string) => {
    setTabs((t) =>
      t.some((x) => x.id === id && x.name !== name) ? t.map((x) => (x.id === id ? { ...x, name } : x)) : t,
    );
  }, []);
  const goHome = useCallback((withNew = false) => {
    setNewProject(withNew);
    setActive(null);
  }, []);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!e.ctrlKey) return;
      if (e.key.toLowerCase() === "w" && active) {
        e.preventDefault();
        close([active]);
      } else if (e.key === "Tab") {
        e.preventDefault();
        const order = [null, ...tabs.map((t) => t.id)];
        const next = order[(order.indexOf(active) + (e.shiftKey ? -1 : 1) + order.length) % order.length];
        if (next) setActive(next);
        else goHome();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [active, tabs, close, goHome]);

  return { tabs, active, setActive, newProject, firstMessages, open, close, renameTab, goHome };
}

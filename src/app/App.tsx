import { Component, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { announceUpdate } from "@/entities/update";
import { Editor } from "@/pages/editor";
import { Home } from "@/pages/home";
import { anyBusy, useAgentSessions } from "@/entities/editor";
import { api, errorMessage, guardClose } from "@/shared/api";
import { Button, EmptyState, toast, ToastHost } from "@/shared/ui";
import { TabStrip, type ProjectTab } from "@/widgets/title-bar";
import "./App.css";

class ErrorBoundary extends Component<{ onReset: () => void; children: ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="app-error">
        <EmptyState
          icon="alert"
          title="문제가 생겼어요"
          description="잠시 뒤 다시 해 보세요. 작업한 내용은 저장되어 있어요."
          action={
            <Button
              kind="primary"
              onClick={() => {
                this.setState({ error: null });
                this.props.onReset();
              }}
            >
              처음 화면으로
            </Button>
          }
        />
      </div>
    );
  }
}

export function App() {
  // Open projects are tabs; `active` is the one shown, or null for the home screen. Only the shown editor is
  // mounted: what a hidden one was doing lives in the backend and the agent sessions, not in the component.
  const [tabs, setTabs] = useState<ProjectTab[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [newProject, setNewProject] = useState(false);
  const [firstMessages, setFirstMessages] = useState<Record<string, string>>({});
  const sessions = useAgentSessions();

  const open = useCallback((id: string, firstMessage?: string, name = "") => {
    if (firstMessage) setFirstMessages((m) => ({ ...m, [id]: firstMessage }));
    setTabs((t) => (t.some((x) => x.id === id) ? t : [...t, { id, name }]));
    setActive(id);
  }, []);
  const close = useCallback((ids: string[]) => {
    setTabs((t) => t.filter((x) => !ids.includes(x.id)));
    setActive((a) => (a && ids.includes(a) ? null : a));
  }, []);
  const named = useCallback((id: string, name: string) => {
    setTabs((t) =>
      t.some((x) => x.id === id && x.name !== name) ? t.map((x) => (x.id === id ? { ...x, name } : x)) : t,
    );
  }, []);
  const home = useCallback((withNew = false) => {
    setNewProject(withNew);
    setActive(null);
  }, []);
  const strip = (
    <TabStrip
      tabs={tabs}
      active={active}
      busy={(id) => sessions.get(id)?.busy ?? false}
      onSelect={(id) => (id ? setActive(id) : home())}
      onClose={(id) => close([id])}
    />
  );

  useEffect(() => {
    const onReject = (e: PromiseRejectionEvent) => toast(errorMessage(e.reason), "error");
    window.addEventListener("unhandledrejection", onReject);
    return () => window.removeEventListener("unhandledrejection", onReject);
  }, []);

  useEffect(() => api.onUpdateAvailable(announceUpdate), []);
  useEffect(() => api.onOpenProject((id) => open(id)), [open]);

  // A turn that ends in a tab that is not shown says so; the shown one is already on screen.
  const wasBusy = useRef(new Set<string>());
  useEffect(() => {
    const busy = new Set([...sessions].filter(([, s]) => s.busy).map(([id]) => id));
    for (const id of wasBusy.current) {
      const tab = tabs.find((t) => t.id === id);
      if (!busy.has(id) && id !== active && tab) toast(`‘${tab.name}’ 작업이 끝났어요`);
    }
    wasBusy.current = busy;
  }, [sessions, tabs, active]);
  // Ctrl+W closes the shown project, Ctrl+Tab and Ctrl+Shift+Tab walk through the tabs, home included.
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
        else home();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [active, tabs, close, home]);
  useEffect(() => guardClose(anyBusy), []);

  return (
    <ErrorBoundary onReset={() => home()}>
      {active === null ? (
        <Home key={String(newProject)} tabs={strip} startWithNew={newProject} onOpen={open} onGone={close} />
      ) : (
        <Editor
          key={active}
          tabs={strip}
          projectId={active}
          firstMessage={firstMessages[active]}
          onName={(name) => named(active, name)}
          onHome={home}
          onOpen={open}
        />
      )}
      <ToastHost />
    </ErrorBoundary>
  );
}

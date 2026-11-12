import { useCallback, useEffect, useRef, useState } from "react";
import { announceUpdate } from "@/entities/update";
import { Editor } from "@/pages/editor";
import { Home } from "@/pages/home";
import { anyBusy, useAgentSessions } from "@/entities/editor";
import {
  discardUnfinishedTutorial,
  finishOnboarding,
  onboarded,
  startTutorial,
  tutorialProject,
} from "@/entities/onboarding";
import { KeyDialog, Tour } from "@/features/onboarding";
import { api, errorMessage, guardClose } from "@/shared/api";
import { toast, ToastHost } from "@/shared/ui";
import { TabStrip, type ProjectTab } from "@/widgets/title-bar";
import { ErrorBoundary } from "./ErrorBoundary";
import { useTabs } from "./useTabs";
import "./App.css";

function useHiddenTabDoneToast(
  sessions: ReturnType<typeof useAgentSessions>,
  tabs: ProjectTab[],
  active: string | null,
) {
  const wasBusy = useRef(new Set<string>());
  useEffect(() => {
    const busy = new Set([...sessions].filter(([, s]) => s.busy).map(([id]) => id));
    for (const id of wasBusy.current) {
      const tab = tabs.find((t) => t.id === id);
      if (!busy.has(id) && id !== active && tab) toast(`‘${tab.name}’ 작업이 끝났어요`);
    }
    wasBusy.current = busy;
  }, [sessions, tabs, active]);
}

export function App() {
  const { tabs, active, setActive, newProject, firstMessages, open, close, renameTab, goHome } = useTabs();
  const sessions = useAgentSessions();
  const [onboarding, setOnboarding] = useState<"tour" | "key" | null>(() => {
    if (onboarded()) return null;
    discardUnfinishedTutorial();
    return "tour";
  });
  const [tutorialStarted, setTutorialStarted] = useState(false);

  useEffect(() => {
    if (!onboarded() || (import.meta.env.DEV && localStorage.getItem("hoist.dev.skip-key"))) return;
    void api.apiKeyStatus().then((status) => {
      if (status === "ok") return;
      if (status === "rejected") toast("저장된 API 키가 맞지 않아요", "error");
      setOnboarding("key");
    });
  }, []);

  useEffect(() => {
    const onReject = (e: PromiseRejectionEvent) => toast(errorMessage(e.reason), "error");
    window.addEventListener("unhandledrejection", onReject);
    return () => window.removeEventListener("unhandledrejection", onReject);
  }, []);

  useEffect(() => api.onUpdateAvailable(announceUpdate), []);
  useEffect(() => api.onOpenProject((id) => open(id)), [open]);
  useHiddenTabDoneToast(sessions, tabs, active);
  useEffect(() => guardClose(anyBusy), []);
  const endTutorial = useCallback(() => {
    const practice = tutorialProject();
    if (practice) {
      close([practice]);
      void api.deleteProject(practice).catch(() => undefined);
    }
    finishOnboarding();
    setOnboarding("key");
  }, [close]);

  const tabStrip = (
    <TabStrip
      tabs={tabs}
      active={active}
      busy={(id) => sessions.get(id)?.busy ?? false}
      onSelect={(id) => (id ? setActive(id) : goHome())}
      onClose={(id) => close([id])}
    />
  );

  return (
    <ErrorBoundary onReset={() => goHome()}>
      {active === null ? (
        <Home
          key={String(newProject)}
          tabs={tabStrip}
          startWithNew={newProject}
          onOpen={open}
          onGone={close}
          onStartTutorial={
            onboarding === "tour" && !tutorialStarted
              ? () => {
                  startTutorial();
                  setTutorialStarted(true);
                }
              : undefined
          }
        />
      ) : (
        <Editor
          key={active}
          tabs={tabStrip}
          projectId={active}
          firstMessage={firstMessages[active]}
          onName={(name) => renameTab(active, name)}
          onHome={goHome}
          onOpen={open}
        />
      )}
      {onboarding === "tour" && <Tour onDone={endTutorial} />}
      {onboarding === "key" && <KeyDialog onDone={() => setOnboarding(null)} />}
      <ToastHost />
    </ErrorBoundary>
  );
}

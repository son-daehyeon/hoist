import { useEffect, useState } from "react";
import { Icon } from "../../src/shared/ui";
import "../../src/shared/ui/tokens.css";
import "./App.css";
import * as api from "./api";
import type { Mode, Plan, Progress } from "./api";
import {
  DoneScreen,
  ErrorScreen,
  InstallingScreen,
  LocationScreen,
  ResourcesErrorScreen,
  UninstallScreen,
  WelcomeScreen,
} from "./screens";

type Screen = "welcome" | "location" | "installing" | "done" | "error" | "resources" | "uninstall";

export function App() {
  const [mode, setMode] = useState<Mode | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [screen, setScreen] = useState<Screen>("welcome");
  const [dir, setDir] = useState("");
  const [desktopShortcut, setDesktopShortcut] = useState(true);
  const [progress, setProgress] = useState<Progress>({ step: 0, percent: 0, detail: "" });
  const [error, setError] = useState("");
  const [resourcesMissing, setResourcesMissing] = useState(false);
  const [keepProjects, setKeepProjects] = useState(true);
  const [removeSettings, setRemoveSettings] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [launchError, setLaunchError] = useState("");

  useEffect(() => {
    api.mode().then((m) => {
      setMode(m);
      if (m.mode === "uninstall") setScreen("uninstall");
    });
    api.plan().then((p) => {
      setPlan(p);
      setDir(p.default_dir);
    });
    const unlisten = api.onProgress(setProgress);
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const isUninstall = mode?.mode === "uninstall";
  const close = () => api.closeWindow();

  const startInstall = async () => {
    setScreen("installing");
    setProgress({ step: 0, percent: 0, detail: "" });
    try {
      const installed = await api.install(dir, desktopShortcut);
      setDir(installed.dir);
      if (installed.resources_error) {
        setError(installed.resources_error);
        setResourcesMissing(installed.resources_missing);
        setScreen("resources");
      } else {
        setScreen("done");
      }
    } catch (e) {
      setError(String(e));
      setScreen("error");
    }
  };

  const retryResources = async () => {
    setScreen("installing");
    setProgress({ step: api.STEPS.length - 1, percent: api.RESOURCES_START_PERCENT, detail: "" });
    try {
      await api.installResources(dir);
      setScreen("done");
    } catch (e) {
      setError(String(e));
      setScreen("resources");
    }
  };

  const runUninstall = async () => {
    setRemoving(true);
    try {
      await api.uninstall(keepProjects, removeSettings);
    } catch (e) {
      setRemoving(false);
      setError(String(e));
      setScreen("error");
    }
  };

  const launch = () => api.launch(dir).catch((e) => setLaunchError(String(e)));

  const browseDir = async () => {
    const picked = await api.pickDir();
    if (picked) setDir(picked);
  };

  return (
    <div className="win">
      <div className="titlebar" data-tauri-drag-region>
        <span className="t-label-s titlebar-title" data-tauri-drag-region>
          {isUninstall ? "Hoist 제거" : "Hoist 설치"}
        </span>
        <button
          className="titlebar-close"
          onClick={close}
          aria-label="닫기"
          disabled={screen === "installing"}
        >
          <Icon name="close" size={10} />
        </button>
      </div>

      {screen === "welcome" && plan && (
        <WelcomeScreen
          plan={plan}
          desktopShortcut={desktopShortcut}
          onDesktopShortcutChange={setDesktopShortcut}
          onInstall={startInstall}
          onChangeLocation={() => setScreen("location")}
        />
      )}

      {screen === "location" && (
        <LocationScreen
          dir={dir}
          onDirChange={setDir}
          onBrowse={browseDir}
          onConfirm={() => setScreen("welcome")}
          onReset={() => plan && setDir(plan.default_dir)}
        />
      )}

      {screen === "installing" && <InstallingScreen progress={progress} />}

      {screen === "done" && (
        <DoneScreen desktopShortcut={desktopShortcut} error={launchError} onLaunch={launch} onClose={close} />
      )}

      {screen === "error" && (
        <ErrorScreen
          isUninstall={isUninstall}
          error={error}
          onRetry={() => setScreen(isUninstall ? "uninstall" : "welcome")}
          onClose={close}
        />
      )}

      {screen === "resources" && (
        <ResourcesErrorScreen
          error={error}
          resourcesMissing={resourcesMissing}
          onRetry={retryResources}
          onDismiss={resourcesMissing ? close : () => setScreen("done")}
        />
      )}

      {screen === "uninstall" && (
        <UninstallScreen
          projectsDir={mode?.projects_dir ?? null}
          keepProjects={keepProjects}
          onKeepProjectsChange={setKeepProjects}
          removeSettings={removeSettings}
          onRemoveSettingsChange={setRemoveSettings}
          removing={removing}
          onUninstall={runUninstall}
          onClose={close}
        />
      )}
    </div>
  );
}

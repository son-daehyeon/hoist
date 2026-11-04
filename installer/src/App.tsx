import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button, Checkbox, Field, Icon, Logo } from "../../src/shared/ui";
import "../../src/shared/ui/tokens.css";
import "./App.css";

type Mode = { mode: "install" | "uninstall"; version: string };
type Plan = {
  default_dir: string;
  size_mb: number;
  existing_dir: string | null;
  webview2: boolean;
  payload_ok: boolean;
  projects_dir: string;
};
type Progress = { step: number; percent: number; detail: string };
type Screen = "welcome" | "location" | "installing" | "done" | "error" | "uninstall";

const STEPS = ["WebView2 런타임 확인", "파일 복사", "바로가기 만들기", "프로그램 등록"];

export default function App() {
  const [mode, setMode] = useState<Mode | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [screen, setScreen] = useState<Screen>("welcome");
  const [dir, setDir] = useState("");
  const [desktop, setDesktop] = useState(true);
  const [progress, setProgress] = useState<Progress>({ step: 0, percent: 0, detail: "" });
  const [error, setError] = useState("");
  const [keepProjects, setKeepProjects] = useState(true);
  const [removeSettings, setRemoveSettings] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<Mode>("mode").then((m) => {
      setMode(m);
      if (m.mode === "uninstall") setScreen("uninstall");
    });
    invoke<Plan>("plan").then((p) => {
      setPlan(p);
      setDir(p.default_dir);
    });
    const un = listen<Progress>("install-progress", (e) => setProgress(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  const close = () => getCurrentWindow().close();

  const startInstall = async () => {
    setScreen("installing");
    setProgress({ step: 0, percent: 0, detail: "" });
    try {
      const installed = await invoke<string>("install", { args: { dir, desktop_shortcut: desktop } });
      setDir(installed);
      setScreen("done");
    } catch (e) {
      setError(String(e));
      setScreen("error");
    }
  };

  const runUninstall = async () => {
    setBusy(true);
    try {
      await invoke("uninstall", { args: { keep_projects: keepProjects, remove_settings: removeSettings } });
    } catch (e) {
      setBusy(false);
      setError(String(e));
      setScreen("error");
    }
  };

  const pick = async () => {
    const picked = await invoke<string | null>("pick_dir");
    if (picked) setDir(picked);
  };

  return (
    <div className="win">
      <div className="titlebar" data-tauri-drag-region>
        <span className="t-label-s titlebar-title" data-tauri-drag-region>
          {mode?.mode === "uninstall" ? "Hoist 제거" : "Hoist 설치"}
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
        <div className="body">
          <AppMark />
          <Heading
            title="Hoist 설치"
            sub="작은 현수막 이미지를 실물 크기로 출력할 수 있는 레이어로 다시 만드는 프로그램이에요."
          />
          <div className="t-micro meta">
            버전 {mode?.version} · 약 {plan.size_mb}MB · 관리자 권한 필요 없음
            {plan.existing_dir ? " · 이미 설치됨 (덮어써요)" : ""}
          </div>
          <div className="actions">
            <Button
              kind="primary"
              size="l"
              className="wide"
              onClick={startInstall}
              disabled={!plan.payload_ok}
            >
              {plan.existing_dir ? "다시 설치" : "설치"}
            </Button>
            <Checkbox checked={desktop} onChange={setDesktop}>
              바탕화면에 바로가기 만들기
            </Checkbox>
          </div>
          <button className="link t-caption" onClick={() => setScreen("location")}>
            설치 위치 바꾸기
          </button>
          {!plan.payload_ok && <div className="t-caption warn">개발용 빌드라 설치 파일이 비어 있어요.</div>}
        </div>
      )}

      {screen === "location" && (
        <div className="body">
          <AppMark />
          <Heading title="설치 위치" sub="바꾸지 않아도 돼요. 기본 위치는 관리자 권한이 필요 없어요." />
          <div className="row">
            <div style={{ width: 300 }}>
              <Field label="폴더" value={dir} onChange={(e) => setDir(e.target.value)} size="l" />
            </div>
            <Button kind="secondary" size="l" icon="folder-open" onClick={pick}>
              찾아보기
            </Button>
          </div>
          <div className="actions">
            <Button kind="primary" size="l" className="wide" onClick={() => setScreen("welcome")}>
              이 위치로 정하기
            </Button>
            <button className="link t-caption" onClick={() => plan && setDir(plan.default_dir)}>
              기본 위치로 되돌리기
            </button>
          </div>
        </div>
      )}

      {screen === "installing" && (
        <div className="body">
          <Heading title="설치하고 있어요" sub="잠시만 기다려 주세요" />
          <ul className="steps">
            {STEPS.map((label, i) => {
              const state = i < progress.step ? "done" : i === progress.step ? "now" : "todo";
              return (
                <li key={label} className={`step step-${state} t-body-l`}>
                  <span className="step-icon">
                    {state === "done" && <Icon name="check" size={14} />}
                    {state === "now" && <Icon name="loader" size={14} className="spin" />}
                  </span>
                  <span className="step-label">{label}</span>
                  {state === "now" && <span className="t-caption step-pct">{progress.percent}%</span>}
                </li>
              );
            })}
          </ul>
          <div className="track">
            <div className="fill" style={{ width: `${progress.percent}%` }} />
          </div>
          <div className="t-micro detail">{progress.detail}</div>
        </div>
      )}

      {screen === "done" && (
        <div className="body">
          <div className="done-icon">
            <Icon name="check" size={28} />
          </div>
          <Heading title="설치가 끝났어요" sub="시작 메뉴와 바탕화면에서 Hoist를 열 수 있어요." />
          <div className="actions">
            <Button kind="primary" size="l" className="wide" onClick={() => invoke("launch", { dir })}>
              실행
            </Button>
            <button className="link t-caption" onClick={close}>
              닫기
            </button>
          </div>
        </div>
      )}

      {screen === "error" && (
        <div className="body">
          <div className="error-icon">
            <Icon name="alert" size={28} />
          </div>
          <Heading title={mode?.mode === "uninstall" ? "지우지 못했어요" : "설치하지 못했어요"} sub={error} />
          <div className="actions">
            <Button
              kind="primary"
              size="l"
              className="wide"
              onClick={() => setScreen(mode?.mode === "uninstall" ? "uninstall" : "welcome")}
            >
              다시 시도
            </Button>
            <button className="link t-caption" onClick={close}>
              닫기
            </button>
          </div>
        </div>
      )}

      {screen === "uninstall" && (
        <div className="body">
          <AppMark />
          <Heading title="Hoist를 지울까요?" sub="프로그램 파일과 바로가기를 지워요." />
          <div className="options">
            <Checkbox checked={keepProjects} onChange={setKeepProjects}>
              프로젝트 폴더(문서\Hoist)는 남겨 두기
            </Checkbox>
            <Checkbox checked={removeSettings} onChange={setRemoveSettings}>
              설정과 API 키도 지우기
            </Checkbox>
          </div>
          <div className="actions">
            <Button
              kind="danger"
              size="l"
              className="wide"
              icon="trash"
              onClick={runUninstall}
              disabled={busy}
            >
              지우기
            </Button>
            <button className="link t-caption" onClick={close}>
              취소
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function AppMark() {
  return <Logo size={56} tile />;
}

function Heading({ title, sub }: { title: string; sub?: string }) {
  return (
    <div className="heading">
      <div className="t-title-l">{title}</div>
      {sub && <div className="t-body-l sub">{sub}</div>}
    </div>
  );
}

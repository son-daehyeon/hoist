import type { ReactNode } from "react";
import { Button, Checkbox, Field, Icon, Logo } from "../../src/shared/ui";
import { STEPS } from "./api";
import type { Plan, Progress } from "./api";

export function WelcomeScreen({
  plan,
  desktopShortcut,
  onDesktopShortcutChange,
  onInstall,
  onChangeLocation,
}: {
  plan: Plan;
  desktopShortcut: boolean;
  onDesktopShortcutChange: (checked: boolean) => void;
  onInstall: () => void;
  onChangeLocation: () => void;
}) {
  return (
    <div className="body">
      <AppMark />
      <Heading title="Hoist 설치" />
      <div className="actions">
        <Button kind="primary" size="l" className="wide" onClick={onInstall} disabled={!plan.payload_ok}>
          {plan.existing_dir ? "다시 설치" : "설치"}
        </Button>
        <Checkbox checked={desktopShortcut} onChange={onDesktopShortcutChange}>
          바탕화면에 바로가기 만들기
        </Checkbox>
      </div>
      <LinkButton onClick={onChangeLocation}>설치 위치 바꾸기</LinkButton>
      {!plan.payload_ok && <div className="t-caption warn">개발용 빌드라 설치 파일이 비어 있어요.</div>}
    </div>
  );
}

export function LocationScreen({
  dir,
  onDirChange,
  onBrowse,
  onConfirm,
  onReset,
}: {
  dir: string;
  onDirChange: (dir: string) => void;
  onBrowse: () => void;
  onConfirm: () => void;
  onReset: () => void;
}) {
  return (
    <div className="body">
      <AppMark />
      <Heading title="설치 위치" />
      <div className="row">
        <div className="dir-field">
          <Field label="폴더" value={dir} onChange={(e) => onDirChange(e.target.value)} size="l" />
        </div>
        <Button kind="secondary" size="l" icon="folder-open" onClick={onBrowse}>
          찾아보기
        </Button>
      </div>
      <div className="actions">
        <Button kind="primary" size="l" className="wide" onClick={onConfirm}>
          이 위치로 정하기
        </Button>
        <LinkButton onClick={onReset}>기본 위치로 되돌리기</LinkButton>
      </div>
    </div>
  );
}

export function InstallingScreen({ progress }: { progress: Progress }) {
  return (
    <div className="body">
      <Heading title="설치하고 있어요" />
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
  );
}

export function DoneScreen({
  desktopShortcut,
  error,
  onLaunch,
  onClose,
}: {
  desktopShortcut: boolean;
  error: string;
  onLaunch: () => void;
  onClose: () => void;
}) {
  return (
    <div className="body">
      <div className="done-icon">
        <Icon name="check" size={28} />
      </div>
      <Heading
        title="설치가 끝났어요"
        sub={
          desktopShortcut
            ? "시작 메뉴와 바탕화면에서 Hoist를 열 수 있어요."
            : "시작 메뉴에서 Hoist를 열 수 있어요."
        }
      />
      <div className="actions">
        <Button kind="primary" size="l" className="wide" onClick={onLaunch}>
          실행
        </Button>
        <LinkButton onClick={onClose}>닫기</LinkButton>
      </div>
      {error && <div className="t-caption warn">{error}</div>}
    </div>
  );
}

export function ErrorScreen({
  isUninstall,
  error,
  onRetry,
  onClose,
}: {
  isUninstall: boolean;
  error: string;
  onRetry: () => void;
  onClose: () => void;
}) {
  return (
    <div className="body">
      <div className="error-icon">
        <Icon name="alert" size={28} />
      </div>
      <Heading title={isUninstall ? "지우지 못했어요" : "설치하지 못했어요"} sub={error} />
      <div className="actions">
        <Button kind="primary" size="l" className="wide" onClick={onRetry}>
          다시 시도
        </Button>
        <LinkButton onClick={onClose}>닫기</LinkButton>
      </div>
    </div>
  );
}

export function ResourcesErrorScreen({
  error,
  resourcesMissing,
  onRetry,
  onDismiss,
}: {
  error: string;
  resourcesMissing: boolean;
  onRetry: () => void;
  onDismiss: () => void;
}) {
  return (
    <div className="body">
      <div className="error-icon">
        <Icon name="alert" size={28} />
      </div>
      <Heading
        title={resourcesMissing ? "글꼴·모델을 받지 못했어요" : "새 글꼴·모델을 받지 못했어요"}
        sub={`${error} ${
          resourcesMissing
            ? "Hoist를 쓰려면 글꼴·모델이 필요해요."
            : "프로그램은 설치됐고, 이전 글꼴·모델로 계속 쓸 수 있어요."
        }`}
      />
      <div className="actions">
        <Button kind="primary" size="l" className="wide" onClick={onRetry}>
          다시 시도
        </Button>
        <LinkButton onClick={onDismiss}>{resourcesMissing ? "닫기" : "나중에 하기"}</LinkButton>
      </div>
    </div>
  );
}

export function UninstallScreen({
  projectsDir,
  keepProjects,
  onKeepProjectsChange,
  removeSettings,
  onRemoveSettingsChange,
  removing,
  onUninstall,
  onClose,
}: {
  projectsDir: string | null;
  keepProjects: boolean;
  onKeepProjectsChange: (checked: boolean) => void;
  removeSettings: boolean;
  onRemoveSettingsChange: (checked: boolean) => void;
  removing: boolean;
  onUninstall: () => void;
  onClose: () => void;
}) {
  return (
    <div className="body">
      <AppMark />
      <Heading title="Hoist를 지울까요?" sub="프로그램 파일과 바로가기를 지워요." />
      <div className="options">
        <Checkbox checked={keepProjects} onChange={onKeepProjectsChange}>
          프로젝트 폴더{projectsDir && `(${projectsDir})`}는 남겨 두기
        </Checkbox>
        <Checkbox checked={removeSettings} onChange={onRemoveSettingsChange}>
          설정과 API 키도 지우기
        </Checkbox>
      </div>
      <div className="actions">
        <Button
          kind="danger"
          size="l"
          className="wide"
          icon="trash"
          onClick={onUninstall}
          disabled={removing}
        >
          지우기
        </Button>
        <LinkButton onClick={onClose}>취소</LinkButton>
      </div>
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

function LinkButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button className="link t-caption" onClick={onClick}>
      {children}
    </button>
  );
}

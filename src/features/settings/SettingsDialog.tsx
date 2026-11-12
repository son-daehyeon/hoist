import { useEffect, useState } from "react";
import { installUpdate, updateErrorMessage } from "@/entities/update";
import { api, errorMessage, type Settings } from "@/shared/api";
import { Button, Dialog, Field, toast } from "@/shared/ui";
import "./settings.css";

type UpdateState =
  | { step: "idle" }
  | { step: "checking" }
  | { step: "latest" }
  | { step: "available" | "installing"; version: string }
  | { step: "error"; message: string };

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [key, setKey] = useState("");
  const [dir, setDir] = useState("");
  const [saving, setSaving] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [update, setUpdate] = useState<UpdateState>({ step: "idle" });

  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        setSettings(s);
        setDir(s.projects_dir);
      })
      .catch(() => toast("설정을 불러오지 못했어요", "error"));
    void api.appVersion().then(setVersion);
  }, []);

  const check = async () => {
    setUpdate({ step: "checking" });
    try {
      const info = await api.updateCheck();
      setUpdate(info ? { step: "available", version: info.version } : { step: "latest" });
    } catch (e) {
      setUpdate({ step: "error", message: updateErrorMessage(e) });
    }
  };

  const install = async (next: string) => {
    setUpdate({ step: "installing", version: next });
    await installUpdate();
    setUpdate({ step: "available", version: next });
  };

  const current = version ?? "–";
  const hasUpdate = update.step === "available" || update.step === "installing";
  const status =
    update.step === "idle"
      ? `현재 ${current}`
      : update.step === "checking"
        ? "확인하는 중"
        : update.step === "latest"
          ? "최신 버전이에요"
          : update.step === "error"
            ? update.message
            : `새 버전 ${update.version}`;

  const save = async () => {
    setSaving(true);
    try {
      await api.setSettings({ ...(key.trim() ? { openai_key: key.trim() } : {}), projects_dir: dir });
      toast("설정을 저장했어요");
      onClose();
    } catch (e) {
      toast(errorMessage(e), "error");
      setSaving(false);
    }
  };

  return (
    <Dialog
      title="설정"
      width={560}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>취소</Button>
          <Button kind="primary" disabled={!settings || saving} onClick={save}>
            저장
          </Button>
        </>
      }
    >
      <div className="form-stack">
        <Field
          size="l"
          type="password"
          label="OpenAI API 키"
          placeholder={settings?.openai_key_set ? "저장됨 · 바꾸려면 입력" : "sk-..."}
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <Field
          size="l"
          label="프로젝트 저장 폴더"
          value={dir}
          readOnly
          action={
            <Button
              size="l"
              onClick={async () => {
                const picked = await api.pickFolder();
                if (picked) setDir(picked);
              }}
            >
              찾아보기
            </Button>
          }
        />
        <div className="update-row">
          <span className="t-label-s c-secondary">업데이트</span>
          <span className="update-status">
            <span className={`t-body-m ${update.step === "error" ? "c-danger" : ""}`}>{status}</span>
            {hasUpdate ? (
              <Button disabled={update.step === "installing"} onClick={() => void install(update.version)}>
                설치
              </Button>
            ) : (
              <Button disabled={update.step === "checking"} onClick={() => void check()}>
                업데이트 확인
              </Button>
            )}
          </span>
        </div>
      </div>
    </Dialog>
  );
}

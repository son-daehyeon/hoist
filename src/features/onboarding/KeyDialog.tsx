import { useState } from "react";
import { api, errorMessage } from "@/shared/api";
import { Button, Dialog, Field, toast } from "@/shared/ui";
import "./onboarding.css";

export function KeyDialog({ onDone }: { onDone: () => void }) {
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const save = async () => {
    setSaving(true);
    try {
      await api.setSettings({ openai_key: key.trim() });
      toast("API 키를 저장했어요");
      onDone();
    } catch (e) {
      toast(errorMessage(e), "error");
      setSaving(false);
    }
  };
  return (
    <Dialog
      title="API 키 넣기"
      footer={
        <Button kind="primary" disabled={saving || !key.trim()} onClick={() => void save()}>
          저장
        </Button>
      }
    >
      <div className="t-body-m ob-key-note">
        OpenAI API 키가 필요해요. platform.openai.com의 API keys에서 만들 수 있어요.
      </div>
      <Field
        size="l"
        type="password"
        label="OpenAI API 키"
        placeholder="sk-..."
        autoFocus
        value={key}
        onChange={(e) => setKey(e.target.value)}
      />
    </Dialog>
  );
}

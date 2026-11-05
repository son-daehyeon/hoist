import { useState } from "react";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import { Field } from "./Field";
import "./QuestionDialog.css";

export function QuestionDialog({
  step,
  total,
  text,
  image,
  options,
  allowFreeText,
  autoLabel,
  onAnswer,
  onAuto,
}: {
  step: number;
  total: number;
  text: string;
  image?: string;
  options: string[];
  allowFreeText: boolean;
  autoLabel: string;
  onAnswer: (answer: string) => void;
  onAuto: () => void;
}) {
  const [draft, setDraft] = useState("");
  const free = draft.trim();

  return (
    <Dialog
      title="에이전트 질문"
      width={image ? 560 : 480}
      footer={
        <Button kind="ghost" icon="sparkles" onClick={onAuto}>
          {autoLabel}
        </Button>
      }
    >
      <div className="bs-qd">
        {total > 1 && (
          <div className="bs-qd-step t-label-m">
            질문 {step} / {total}
          </div>
        )}
        <div className="t-body-m bs-qd-text">{text}</div>
        {image && (
          <div className="bs-qd-image">
            <img src={image} alt="질문과 관련된 원본 부분" />
          </div>
        )}
        {options.length > 0 && (
          <div className="bs-qd-options">
            {options.map((o, i) => (
              <Button key={o} size="l" kind={i === 0 ? "primary" : "secondary"} onClick={() => onAnswer(o)}>
                {o}
              </Button>
            ))}
          </div>
        )}
        {allowFreeText && (
          <div className="bs-qd-free">
            <Field
              size="l"
              aria-label="직접 답하기"
              placeholder="직접 적어 주세요"
              autoFocus={options.length === 0}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.nativeEvent.isComposing && free) onAnswer(free);
              }}
            />
            <Button
              size="l"
              kind={options.length ? "secondary" : "primary"}
              disabled={!free}
              onClick={() => onAnswer(free)}
            >
              답하기
            </Button>
          </div>
        )}
      </div>
    </Dialog>
  );
}

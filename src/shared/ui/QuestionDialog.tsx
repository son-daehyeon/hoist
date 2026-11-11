import { useState } from "react";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import { Field } from "./Field";
import { Md } from "./Misc";
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
        autoLabel ? (
          <Button kind="ghost" icon="sparkles" onClick={onAuto}>
            {autoLabel}
          </Button>
        ) : undefined
      }
    >
      <div className="bs-qd">
        {total > 1 && (
          <div className="bs-qd-step t-label-m">
            질문 {step} / {total}
          </div>
        )}
        <div className="t-body-m bs-qd-text">
          <Md text={text} />
        </div>
        {image && (
          <div className="bs-qd-image">
            <img src={image} alt="질문과 관련된 원본 부분" />
          </div>
        )}
        {options.length > 0 && (
          <div
            className={`bs-qd-options ${options.length === 2 && !allowFreeText && options.every((o) => o.length <= 8) ? "is-pair" : ""}`}
          >
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
              placeholder={options.length === 1 ? "다르면 바른 내용을 적어 주세요" : "직접 적어 주세요"}
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

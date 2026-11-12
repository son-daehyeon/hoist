import type { ReactNode } from "react";
import { Button, Icon, ProgressLine, noAutofill, type IconName } from "@/shared/ui";

export function ContextTag({
  icon,
  label,
  onRemove,
}: {
  icon: IconName;
  label: string;
  onRemove?: () => void;
}) {
  return (
    <span className="bs-ctx-tag t-micro">
      <Icon name={icon} size={12} />
      <span className="bs-ctx-label">{label}</span>
      {onRemove && (
        <button type="button" aria-label="빼기" onClick={onRemove}>
          <Icon name="close" size={10} />
        </button>
      )}
    </span>
  );
}

export function Composer({
  value,
  onChange,
  onSend,
  onStop,
  tags,
  lock,
  leading,
}: {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  onStop: () => void;
  tags?: ReactNode;
  lock: "busy" | "question" | null;
  leading?: ReactNode;
}) {
  const placeholder =
    lock === "busy"
      ? "작업이 끝나면 입력할 수 있어요"
      : lock === "question"
        ? "위에서 골라 주세요"
        : "무엇을 고칠까요?";
  const canSend = lock === null && value.trim() !== "";
  return (
    <div className="bs-composer">
      {lock === "busy" && (
        <div className="bs-composer-band">
          <ProgressLine value={null} />
        </div>
      )}
      {tags && <div className="bs-composer-tags">{tags}</div>}
      <textarea
        {...noAutofill}
        className="t-body-m"
        rows={1}
        value={lock ? "" : value}
        disabled={lock !== null}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            if (canSend) onSend();
          }
        }}
      />
      <div className="bs-composer-bar">
        {leading}
        <span className="bs-composer-spacer" />
        {lock === "busy" ? (
          <Button icon="stop" onClick={onStop}>
            중단
          </Button>
        ) : lock === null ? (
          <button
            type="button"
            className="bs-send-btn"
            aria-label="보내기"
            disabled={!canSend}
            onClick={onSend}
          >
            <Icon name="send" size={14} />
          </button>
        ) : null}
      </div>
    </div>
  );
}

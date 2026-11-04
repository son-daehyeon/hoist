import { useState, type ReactNode } from "react";
import { Button } from "./Button";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import { ProgressLine, Tag } from "./Misc";
import "./Assistant.css";

export function Message({
  role,
  working = false,
  children,
}: {
  role: "user" | "agent";
  working?: boolean;
  children: ReactNode;
}) {
  if (role === "user") return <div className="bs-msg-user t-body-m">{children}</div>;
  return (
    <div className="bs-msg-agent">
      <span className={`bs-msg-avatar ${working ? "is-working" : ""}`}>
        <Icon name="sparkles" size={12} />
      </span>
      <div className="bs-msg-body t-body-m">{children}</div>
    </div>
  );
}

type ActionStatus = "running" | "done" | "failed" | "todo";

const STATUS_ICON: Record<ActionStatus, IconName> = {
  done: "check",
  running: "loader",
  todo: "circle",
  failed: "alert",
};

type ActionRow = { id: string; label: string; status: ActionStatus; detail?: string | null };

function mergeRuns(items: ActionRow[]) {
  const rows: (ActionRow & { count: number })[] = [];
  for (const it of items) {
    const prev = rows.at(-1);
    if (prev?.label === it.label) rows[rows.length - 1] = { ...it, id: prev.id, count: prev.count + 1 };
    else rows.push({ ...it, count: 1 });
  }
  return rows;
}

export function Caret() {
  return <span className="bs-caret" aria-hidden />;
}

export function ActionList({ items }: { items: ActionRow[] }) {
  const rows = mergeRuns(items);
  const done = rows.filter((i) => i.status === "done").length;
  return (
    <div className="bs-actions">
      <div className="bs-actions-head">
        <span className="t-label-s">작업</span>
        <span className="t-caption c-tertiary">
          {done}/{rows.length}
        </span>
      </div>
      {rows.map((it) => (
        <div key={it.id} className={`bs-action-row t-caption is-${it.status}`}>
          <Icon name={STATUS_ICON[it.status]} size={14} />
          <span className="bs-action-label">{it.label}</span>
          {it.detail && <span className="bs-action-detail c-tertiary">{it.detail}</span>}
          {it.count > 1 && <Tag>×{it.count}</Tag>}
        </div>
      ))}
    </div>
  );
}

export function Thinking({ text, seconds }: { text: string; seconds: number | null }) {
  const [open, setOpen] = useState<boolean | null>(null);
  const streaming = seconds === null;
  const shown = open ?? streaming;
  return (
    <div className="bs-thinking">
      <button type="button" className="bs-thinking-head t-caption" onClick={() => setOpen(!shown)}>
        <Icon name={shown ? "chevron-down" : "chevron-right"} size={12} />
        {streaming ? "생각하는 중…" : `생각 (${seconds}초)`}
      </button>
      {shown && (
        <div className="bs-thinking-body t-caption c-secondary">
          {text}
          {streaming && <Caret />}
        </div>
      )}
    </div>
  );
}

export function CompareReport({
  originalSrc,
  workSrc,
  stats,
}: {
  originalSrc: string;
  workSrc: string;
  stats: string;
}) {
  return (
    <div className="bs-report">
      <div className="bs-report-thumbs">
        <figure>
          <figcaption className="t-micro c-tertiary">원본</figcaption>
          <img src={originalSrc} alt="원본" />
        </figure>
        <figure>
          <figcaption className="t-micro c-tertiary">작업본</figcaption>
          <img src={workSrc} alt="작업본" />
        </figure>
      </div>
      <div className="t-caption c-secondary">{stats}</div>
    </div>
  );
}

export function Question({
  text,
  options,
  answer,
  onAnswer,
}: {
  text: string;
  options: string[];
  answer?: string;
  onAnswer: (option: string) => void;
}) {
  return (
    <div className="bs-question">
      <div className="t-body-m">{text}</div>
      {options.map((o, i) => (
        <Button
          key={o}
          kind={(answer ? answer === o : i === 0) ? "primary" : "secondary"}
          disabled={answer !== undefined && answer !== o}
          onClick={() => answer === undefined && onAnswer(o)}
        >
          {o}
        </Button>
      ))}
    </div>
  );
}

export function EditSummary({ title, lines }: { title: string; lines: string[] }) {
  return (
    <div className="bs-edit-summary">
      <div className="t-label-s">{title}</div>
      {lines.map((l, i) => (
        <div key={i} className="t-caption c-secondary">
          · {l}
        </div>
      ))}
    </div>
  );
}

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

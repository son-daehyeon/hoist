import { useEffect, useState, type ReactNode } from "react";
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
type Target = ActionRow & { target: string };

const KIND_SEP = " · ";

function groupByKind(items: ActionRow[]) {
  const groups = new Map<string, Target[]>();
  for (const it of items) {
    const at = it.label.indexOf(KIND_SEP);
    const kind = at < 0 ? it.label : it.label.slice(0, at);
    const target = at < 0 ? "" : it.label.slice(at + KIND_SEP.length);
    groups.set(kind, [...(groups.get(kind) ?? []), { ...it, target }]);
  }
  return [...groups];
}

function targetText({ target, detail }: Target) {
  if (!detail) return target;
  if (!target || detail.startsWith(target)) return detail;
  return `${target}${KIND_SEP}${detail}`;
}

function groupStatus(items: Target[]): ActionStatus {
  if (items.some((i) => i.status === "running")) return "running";
  if (items.some((i) => i.status === "failed")) return "failed";
  return items.every((i) => i.status === "done") ? "done" : (items.at(-1)?.status ?? "todo");
}

export function Caret() {
  return <span className="bs-caret" aria-hidden />;
}

export function ActionList({ items }: { items: ActionRow[] }) {
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const toggle = (kind: string) =>
    setOpen((cur) => {
      const next = new Set(cur);
      if (!next.delete(kind)) next.add(kind);
      return next;
    });

  return (
    <div className="bs-actions">
      {groupByKind(items).map(([kind, group]) => {
        if (group.length === 1) {
          const it = group[0];
          return (
            <div key={it.id} className={`bs-action-row t-caption is-${it.status}`}>
              <Icon name={STATUS_ICON[it.status]} size={14} />
              <span className="bs-action-label">{it.label}</span>
              {it.detail && <span className="bs-action-detail c-tertiary">{it.detail}</span>}
            </div>
          );
        }
        const status = groupStatus(group);
        const running = group.find((i) => i.status === "running");
        const failed = group.filter((i) => i.status === "failed").length;
        const expanded = open.has(kind);
        return (
          <div key={kind} className={`bs-action-group ${expanded ? "is-open" : ""}`}>
            <button
              type="button"
              className={`bs-action-row bs-action-toggle t-caption is-${status}`}
              aria-expanded={expanded}
              onClick={() => toggle(kind)}
            >
              <Icon name={STATUS_ICON[status]} size={14} />
              <span className="bs-action-label">{kind}</span>
              <Tag>×{group.length}</Tag>
              <span className="bs-action-detail c-tertiary">{running ? targetText(running) : ""}</span>
              {failed > 0 && <Tag tone="danger">실패 {failed}</Tag>}
              <Icon name="chevron-right" size={12} className="bs-action-chevron" />
            </button>
            {expanded && (
              <div className="bs-action-items">
                {group.map((it) => (
                  <div key={it.id} className={`bs-action-row t-caption is-${it.status}`}>
                    <Icon name={STATUS_ICON[it.status]} size={12} />
                    <span className="bs-action-detail">{targetText(it) || it.label}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function useElapsed(started: number, running: boolean) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [running]);
  return Math.max(0, Math.floor((now - started) / 1000));
}

export function Thinking({ started, seconds }: { started: number; seconds: number | null }) {
  const elapsed = useElapsed(started, seconds === null);
  return (
    <div className="bs-thinking t-caption c-tertiary">
      {seconds === null ? `생각하는 중 · ${elapsed}초` : `생각 · ${seconds}초`}
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

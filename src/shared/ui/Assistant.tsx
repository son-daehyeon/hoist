import { useEffect, useState, type ReactNode } from "react";
import { Button } from "./Button";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import { ProgressLine, Tag } from "./Misc";
import "./Assistant.css";
import { noAutofill } from "./noAutofill";

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

type ActionStatus = "running" | "done" | "failed" | "stopped" | "todo";

type ActionRow = { id: string; label: string; status: ActionStatus; detail?: string | null };
type Target = ActionRow & { target: string };

const KIND_SEP = " · ";

// Chats saved before 2026-11-06 carry layer ids, raw field names and pixel sizes in their action text.
const tidy = (s: string) =>
  s
    .replace(/ · [a-z_.]+(, [a-z_.]+)*$/, "")
    .replace(/, \d+px$/, "")
    .replace(/ 1\/\d+ 영역/, " 일부")
    .replace(/^'(.*)'$/, "$1");

function groupByKind(items: ActionRow[]) {
  const groups = new Map<string, Target[]>();
  for (const it of items) {
    const at = it.label.indexOf(KIND_SEP);
    const kind = at < 0 ? it.label : it.label.slice(0, at);
    const target = at < 0 ? "" : it.label.slice(at + KIND_SEP.length);
    const row = { ...it, target: /^L\d+$/.test(target) ? "" : target, detail: it.detail && tidy(it.detail) };
    groups.set(kind, [...(groups.get(kind) ?? []), row]);
  }
  return [...groups];
}

/** One action on its own row: what it touched and what it did there. */
function fullText({ target, detail }: Target) {
  if (!detail) return target;
  if (!target || detail.startsWith(target)) return detail;
  return `${target}${KIND_SEP}${detail}`;
}

/** In a row that lists several actions: only what each one touched. */
const shortText = ({ target, detail }: Target) => target || (detail ?? "").split(KIND_SEP)[0];

function groupStatus(items: Target[]): ActionStatus {
  if (items.some((i) => i.status === "running")) return "running";
  if (items.some((i) => i.status === "failed")) return "failed";
  if (items.some((i) => i.status === "stopped")) return "stopped";
  return items.every((i) => i.status === "done") ? "done" : (items.at(-1)?.status ?? "todo");
}

export function Caret() {
  return <span className="bs-caret" aria-hidden />;
}

/**
 * One round of work as a quiet log, two columns: what was done on the left, what it touched on the right.
 * A long list wraps inside its own column, so every row still starts with its label, and a name is never
 * broken across lines. Status only shows when it needs attention (a spinner while running, red when failed).
 */
export function ActionList({ items }: { items: ActionRow[] }) {
  return (
    <div className="bs-actions t-caption">
      {groupByKind(items).map(([kind, group]) => {
        const status = groupStatus(group);
        const failed = group.filter((i) => i.status === "failed").length;
        const stopped = group.filter((i) => i.status === "stopped").length;
        const many = group.length > 1;
        const texts = group
          .map((it) => ({ it, text: many ? shortText(it) : fullText(it) }))
          .filter((x) => x.text);
        return (
          <div key={kind} className={`bs-action is-${status}`}>
            {(status === "running" || status === "failed") && (
              <span className="bs-action-mark" aria-hidden>
                <Icon name={status === "running" ? "loader" : "alert"} size={12} />
              </span>
            )}
            <span className="bs-action-label">{kind}</span>
            <span className={`bs-action-text ${many ? "is-list" : ""}`}>
              {many && <span className="bs-action-count">{group.length}번</span>}
              {texts.map(({ it, text }, i) => (
                <span key={it.id}>
                  <span className={`bs-action-item is-${it.status}`}>
                    {text}
                    {i < texts.length - 1 ? "," : ""}
                  </span>{" "}
                </span>
              ))}
            </span>
            {failed > 0 && group.length > 1 && <Tag tone="danger">실패 {failed}</Tag>}
            {stopped > 0 && <Tag>중단 {stopped}</Tag>}
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

export function Thinking() {
  return <div className="bs-thinking t-caption">생각하는 중</div>;
}

const duration = (s: number) => (s < 60 ? `${s}초` : `${Math.round(s / 60)}분`);

export function TurnSummary({
  parts,
  thinkingSec,
  thinkingSince,
  cost,
  failed,
  stopped,
  expanded,
  onToggle,
}: {
  parts: string[];
  thinkingSec: number;
  thinkingSince: number | null;
  cost: string | null;
  failed: number;
  stopped: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const elapsed = useElapsed(thinkingSince ?? 0, thinkingSince !== null);
  const think = thinkingSec + (thinkingSince === null ? 0 : elapsed);
  const text = [...parts, think > 0 || thinkingSince !== null ? `생각 ${duration(think)}` : "", cost ?? ""]
    .filter(Boolean)
    .join(" · ");
  return (
    <button
      type="button"
      className={`bs-turn-summary t-caption ${expanded ? "is-open" : ""}`}
      aria-expanded={expanded}
      onClick={onToggle}
    >
      <Icon name="chevron-right" size={12} className="bs-turn-chevron" />
      <span className="bs-turn-text">{text}</span>
      {failed > 0 && <Tag tone="danger">실패 {failed}</Tag>}
      {stopped > 0 && <Tag>중단 {stopped}</Tag>}
    </button>
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

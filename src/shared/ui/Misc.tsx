import type { ReactNode } from "react";
import { parseMarkdown } from "./markdown";
import { Icon } from "./Icon";
import type { IconName } from "./icons";
import "./Misc.css";

export function ProgressLine({ value }: { value: number | null }) {
  return (
    <div className="bs-progress" role="progressbar">
      <div
        className={`bs-progress-bar ${value === null ? "is-indeterminate" : ""}`}
        style={value === null ? undefined : { width: `${Math.max(4, Math.min(100, value * 100))}%` }}
      />
    </div>
  );
}

export function Tag({
  tone = "tertiary",
  children,
}: {
  tone?: "tertiary" | "link" | "danger";
  children: ReactNode;
}) {
  return <span className={`bs-tag bs-tag-${tone} t-micro`}>{children}</span>;
}

export function EmptyState({
  icon = "image",
  title,
  description,
  action,
}: {
  icon?: IconName;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="bs-empty">
      <Icon name={icon} size={24} />
      <div className="t-title-s">{title}</div>
      {description && <div className="t-caption c-tertiary">{description}</div>}
      {action}
    </div>
  );
}

/** Agent text with its light markdown (bold, italic, code, bullets) drawn instead of shown as symbols. */
export function Md({ text }: { text: string }) {
  return (
    <>
      {parseMarkdown(text).map((s, i) =>
        s.kind === "bold" ? (
          <strong key={i}>{s.text}</strong>
        ) : s.kind === "italic" ? (
          <em key={i}>{s.text}</em>
        ) : s.kind === "code" ? (
          <code key={i} className="bs-md-code">
            {s.text}
          </code>
        ) : (
          s.text
        ),
      )}
    </>
  );
}

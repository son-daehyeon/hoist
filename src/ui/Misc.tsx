import type { ReactNode } from "react";
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

export function Tag({ tone = "tertiary", children }: { tone?: "tertiary" | "link" | "danger"; children: ReactNode }) {
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

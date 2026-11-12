import type { ActionItem } from "@/shared/api";
import { Icon, Md, Tag } from "@/shared/ui";

type Target = ActionItem & { target: string };

const KIND_SEP = " · ";

// Chats saved before 2026-11-06 carry layer ids, raw field names and pixel sizes in their action text.
const tidy = (s: string) =>
  s
    .replace(/ · [a-z_.]+(, [a-z_.]+)*$/, "")
    .replace(/, \d+px$/, "")
    .replace(/ 1\/\d+ 영역/, " 일부")
    .replace(/^'(.*)'$/, "$1");

function groupByKind(items: ActionItem[]) {
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

function fullText({ target, detail }: Target) {
  if (!detail) return target;
  if (!target || detail.startsWith(target)) return detail;
  return `${target}${KIND_SEP}${detail}`;
}

const lineText = ({ target, detail }: Target) => target || (detail ?? "");

function groupStatus(items: Target[]): ActionItem["status"] {
  if (items.some((i) => i.status === "running")) return "running";
  if (items.some((i) => i.status === "failed")) return "failed";
  if (items.some((i) => i.status === "stopped")) return "stopped";
  return "done";
}

export function ActionList({ items }: { items: ActionItem[] }) {
  return (
    <div className="bs-actions t-caption">
      {groupByKind(items).map(([kind, group]) => {
        const status = groupStatus(group);
        const failed = group.filter((i) => i.status === "failed").length;
        const stopped = group.filter((i) => i.status === "stopped").length;
        const many = group.length > 1;
        const texts = group
          .map((it) => ({ it, text: many ? lineText(it) : fullText(it) }))
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
              {many && texts.length === 0 && <span className="bs-action-count">{group.length}번</span>}
              {texts.map(({ it, text }) => (
                <span key={it.id} className={`bs-action-item is-${it.status}`}>
                  <Md text={text} />
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

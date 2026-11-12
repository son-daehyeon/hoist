import { useEffect, useState } from "react";
import { Icon, Tag } from "@/shared/ui";

function useElapsed(started: number, running: boolean) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [running]);
  return Math.max(0, Math.floor((now - started) / 1000));
}

const duration = (s: number) => (s < 60 ? `${Math.round(s)}초` : `${Math.round(s / 60)}분`);

export function TurnSummary({
  parts,
  thinkingSec,
  thinkingSince,
  failed,
  stopped,
  expanded,
  onToggle,
}: {
  parts: string[];
  thinkingSec: number;
  thinkingSince: number | null;
  failed: number;
  stopped: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const elapsed = useElapsed(thinkingSince ?? 0, thinkingSince !== null);
  const think = thinkingSec + (thinkingSince === null ? 0 : elapsed);
  const text = [...parts, think > 0 || thinkingSince !== null ? `생각 ${duration(think)}` : ""]
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

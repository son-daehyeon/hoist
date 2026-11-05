import type { MouseEvent } from "react";
import { Icon } from "./Icon";
import "./Project.css";

function Thumb({ src, className }: { src?: string; className: string }) {
  return <div className={className}>{src && <img src={src} alt="" draggable={false} />}</div>;
}

const contextMenu = (onMore: (anchor: DOMRect) => void) => (e: MouseEvent) => {
  e.preventDefault();
  e.stopPropagation();
  onMore(new DOMRect(e.clientX, e.clientY, 0, 0));
};

function MoreButton({ onMore }: { onMore: (anchor: DOMRect) => void }) {
  return (
    <button
      type="button"
      className="bs-more-btn"
      aria-label="더 보기"
      onClick={(e: MouseEvent<HTMLButtonElement>) => {
        e.stopPropagation();
        onMore(e.currentTarget.getBoundingClientRect());
      }}
    >
      <Icon name="more" size={16} />
    </button>
  );
}

export type TileStatus = { kind: "working" | "done"; label?: string | null };

export function ProjectTile({
  title,
  meta,
  thumbnail,
  status,
  onOpen,
  onMore,
}: {
  title: string;
  meta: string;
  thumbnail?: string;
  status?: TileStatus;
  onOpen: () => void;
  onMore: (anchor: DOMRect) => void;
}) {
  return (
    <div className="bs-tile" onClick={onOpen} onContextMenu={contextMenu(onMore)}>
      <div className="bs-tile-thumb-wrap">
        <Thumb src={thumbnail} className="bs-tile-thumb" />
        {status && (
          <span className={`bs-tile-badge t-micro is-${status.kind}`}>
            {status.kind === "working" ? "작업 중" : "완료"}
          </span>
        )}
        <MoreButton onMore={onMore} />
      </div>
      <div className="bs-tile-title t-label-m">{title}</div>
      <div className="t-caption c-tertiary">{meta}</div>
      {status?.kind === "working" && status.label && (
        <div className="bs-tile-status t-caption c-secondary">{status.label}</div>
      )}
    </div>
  );
}

export function ProjectListRow({
  title,
  size,
  date,
  thumbnail,
  onOpen,
  onMore,
}: {
  title: string;
  size: string;
  date: string;
  thumbnail?: string;
  onOpen: () => void;
  onMore: (anchor: DOMRect) => void;
}) {
  return (
    <div className="bs-list-row" onClick={onOpen} onContextMenu={contextMenu(onMore)}>
      <Thumb src={thumbnail} className="bs-list-thumb" />
      <span className="t-label-m bs-ellipsis">{title}</span>
      <span className="t-caption c-secondary">{size}</span>
      <span className="t-caption c-secondary">{date}</span>
      <MoreButton onMore={onMore} />
    </div>
  );
}

export function VersionRow({
  time,
  summary,
  thumbnail,
  current,
  onClick,
}: {
  time: string;
  summary: string;
  thumbnail?: string;
  current: boolean;
  onClick: () => void;
}) {
  return (
    <button type="button" className={`bs-version-row ${current ? "is-current" : ""}`} onClick={onClick}>
      <Thumb src={thumbnail} className="bs-version-thumb" />
      <span className="bs-version-text">
        <span className="t-label-s">{time}</span>
        <span className="t-caption c-secondary bs-ellipsis">{summary}</span>
      </span>
    </button>
  );
}

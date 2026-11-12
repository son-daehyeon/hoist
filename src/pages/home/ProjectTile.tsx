import type { MouseEvent } from "react";
import { Icon } from "@/shared/ui";
import "./project-tile.css";

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
        <div className="bs-tile-thumb">{thumbnail && <img src={thumbnail} alt="" draggable={false} />}</div>
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

export function FolderTile({
  title,
  meta,
  onOpen,
  onMore,
}: {
  title: string;
  meta: string;
  onOpen: () => void;
  onMore: (anchor: DOMRect) => void;
}) {
  return (
    <div className="bs-tile" onClick={onOpen} onContextMenu={contextMenu(onMore)}>
      <div className="bs-tile-thumb-wrap">
        <div className="bs-tile-thumb is-folder">
          <Icon name="folder" size={40} />
        </div>
        <MoreButton onMore={onMore} />
      </div>
      <div className="bs-tile-title t-label-m">{title}</div>
      <div className="t-caption c-tertiary">{meta}</div>
    </div>
  );
}

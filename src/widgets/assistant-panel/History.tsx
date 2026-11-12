import { useState } from "react";
import { useEd } from "@/entities/editor";
import { pngSrc, timeLabel } from "@/shared/lib";
import { Button } from "@/shared/ui";

function VersionRow({
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
      <div className="bs-version-thumb">{thumbnail && <img src={thumbnail} alt="" draggable={false} />}</div>
      <span className="bs-version-text">
        <span className="t-label-s">{time}</span>
        <span className="t-caption c-secondary bs-ellipsis">{summary}</span>
      </span>
    </button>
  );
}

export function History() {
  const { versions, restore, busy } = useEd();
  const [picked, setPicked] = useState<number | null>(null);
  const current = versions[0]?.seq;
  const sel = picked ?? current;
  return (
    <>
      <div className="as-body as-history">
        {versions.map((v) => (
          <VersionRow
            key={v.seq}
            time={v.seq === current ? "지금" : timeLabel(v.ts)}
            summary={`${v.summary} · ${v.actor === "agent" ? "에이전트" : "나"}`}
            thumbnail={pngSrc(v.thumbnail_png)}
            current={v.seq === sel}
            onClick={() => setPicked(v.seq)}
          />
        ))}
      </div>
      <div className="as-history-foot">
        <Button
          icon="undo"
          className="as-wide"
          disabled={busy || sel === undefined || sel === current}
          onClick={() => {
            if (sel === undefined) return;
            restore(sel);
            setPicked(null);
          }}
        >
          이 버전으로 되돌리기
        </Button>
      </div>
    </>
  );
}

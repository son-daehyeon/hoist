import { useState } from "react";
import { useEd } from "@/entities/editor";
import { api, errorMessage, type FitMode } from "@/shared/api";
import { Icon, Menu, toast } from "@/shared/ui";

const SHORT: Record<FitMode, string> = {
  ask: "배치: 먼저 물어보기",
  extend_background: "배치: 배경 늘리기",
  relayout: "배치: 요소 재배치",
};

const LONG: Record<FitMode, string> = {
  ask: "에이전트가 먼저 물어보기",
  extend_background: "배경만 늘려서 채우기",
  relayout: "현수막 비율에 맞게 요소 재배치",
};

export function FitModePicker() {
  const { projectId, project, refresh, locked } = useEd();
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const mode = project?.fit_mode ?? "ask";

  const pick = (next: FitMode) =>
    api
      .setFitMode(projectId, next)
      .then(() => refresh())
      .catch((e) => toast(errorMessage(e), "error"));

  return (
    <span onPointerDown={(e) => e.stopPropagation()}>
      <button
        type="button"
        className="cv-fit t-micro"
        disabled={locked}
        onClick={(e) => setAnchor(e.currentTarget.getBoundingClientRect())}
      >
        {SHORT[mode]}
        <Icon name="chevron-down" size={10} />
      </button>
      {anchor && (
        <Menu
          anchor={anchor}
          width={220}
          onClose={() => setAnchor(null)}
          items={(Object.keys(LONG) as FitMode[]).map((m) => ({
            label: LONG[m],
            checked: m === mode,
            onSelect: () => void pick(m),
          }))}
        />
      )}
    </span>
  );
}

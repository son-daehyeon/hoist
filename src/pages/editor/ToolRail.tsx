import { useEd } from "@/entities/editor";
import { ToolButton } from "@/shared/ui";
import { ShapeToolButton } from "@/widgets/canvas";
import { TOOLS } from "./tools";

const COMPARE_ON = 0.5;

export function ToolRail() {
  const { tool, setTool, compare, setCompare } = useEd();
  return (
    <nav className="ed-rail">
      {TOOLS.map((t) =>
        t.tool === "shape" ? (
          <ShapeToolButton key={t.tool} />
        ) : (
          <ToolButton
            key={t.tool}
            icon={t.icon}
            label={t.label}
            shortcut={t.key}
            active={tool === t.tool}
            onClick={() => setTool(t.tool)}
          />
        ),
      )}
      <ToolButton
        icon="compare"
        label="비교"
        active={compare > 0}
        onClick={() => setCompare(compare > 0 ? 0 : COMPARE_ON)}
      />
    </nav>
  );
}

import { useContext } from "react";
import { useEd } from "@/entities/editor";
import { ContextMenu } from "@/shared/ui";
import { ContextCommandsCtx, contextMenuItems } from "./contextMenu";

export function ObjectContextMenu({
  x,
  y,
  blank,
  onClose,
}: {
  x: number;
  y: number;
  blank: boolean;
  onClose: () => void;
}) {
  const ed = useEd();
  const commands = useContext(ContextCommandsCtx);
  if (!commands) return null;
  return <ContextMenu x={x} y={y} items={contextMenuItems(ed, commands, { blank })} onClose={onClose} />;
}

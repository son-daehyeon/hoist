import { useMemo } from "react";
import { Menu, type MenuItem } from "./Menu";

export function ContextMenu({
  x,
  y,
  items,
  onClose,
  width = 220,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
  width?: number;
}) {
  const anchor = useMemo(() => new DOMRect(x, y, 0, 0), [x, y]);
  return <Menu anchor={anchor} items={items} onClose={onClose} width={width} point />;
}

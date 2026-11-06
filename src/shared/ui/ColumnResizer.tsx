import "./ColumnResizer.css";
import type { useColumnWidth } from "./useColumnWidth";

export function ColumnResizer({
  column,
  label,
}: {
  column: ReturnType<typeof useColumnWidth>;
  label: string;
}) {
  return (
    <div
      className={`bs-col-resize is-${column.edge} ${column.resizing ? "is-active" : ""}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      title="끌어서 너비 바꾸기"
      {...column.handle}
    />
  );
}

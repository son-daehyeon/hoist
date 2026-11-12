import { useEffect, useRef, type DragEvent } from "react";
import { onFileDrop } from "@/shared/api";

export function useImageDrop(importImages: (paths: string[]) => Promise<unknown>, blocked: boolean) {
  const drop = useRef((_paths: string[]) => {});
  useEffect(() => {
    drop.current = (paths) => {
      if (!blocked) void importImages(paths);
    };
  });
  useEffect(() => onFileDrop((paths) => drop.current(paths)), []);
  return {
    onDragOver: (e: DragEvent) => {
      if (e.dataTransfer.types.includes("Files")) e.preventDefault();
    },
    onDrop: (e: DragEvent) => {
      if (!e.dataTransfer.files.length) return;
      e.preventDefault();
      drop.current([...e.dataTransfer.files].map((f) => f.name));
    },
  };
}

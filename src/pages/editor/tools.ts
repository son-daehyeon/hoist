import type { Tool } from "@/entities/editor";
import type { IconName } from "@/shared/ui";

export const TOOLS: { tool: Tool; icon: IconName; label: string; key: string }[] = [
  { tool: "select", icon: "cursor", label: "선택", key: "V" },
  { tool: "hand", icon: "hand", label: "이동", key: "H" },
  { tool: "text", icon: "type", label: "글자", key: "T" },
  { tool: "shape", icon: "square", label: "도형", key: "R" },
  { tool: "pen", icon: "pen", label: "펜", key: "P" },
  { tool: "capture", icon: "crop", label: "캡처", key: "C" },
];

export const KEY_TOOL: Record<string, Tool> = Object.fromEntries(
  TOOLS.map((t) => [t.key.toLowerCase(), t.tool]),
);

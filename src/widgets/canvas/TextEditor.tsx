import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ensureFont, useFonts } from "@/entities/font";
import type { Layer } from "@/shared/api";
import { noAutofill } from "@/shared/ui";
import type { Params } from "./drag";
import type { Rect } from "./viewport";

const colorOf = (l: Layer) => {
  const f = l.fills[0];
  return f?.type === "solid"
    ? f.color
    : f?.type === "linear_gradient"
      ? (f.stops[0]?.color ?? "#000")
      : "#000";
};

export function TextEditor({
  layer,
  p,
  z,
  pane,
  outline,
  pending,
  onClose,
}: {
  layer: Layer;
  p: Params;
  z: number;
  pane: Rect;
  outline: boolean;
  pending: boolean;
  onClose: (text: string | null) => void;
}) {
  const k = layer.kind.type === "text" ? layer.kind : null;
  const [value, setValue] = useState(k?.text ?? "");
  const ref = useRef<HTMLTextAreaElement>(null);
  const done = useRef(false);
  const fonts = useFonts();
  const font = fonts?.find((f) => f.family === k?.font_family);
  const weight = k?.font_weight ?? 400;
  useEffect(() => {
    if (font) ensureFont(font, weight);
  }, [font, weight]);
  useLayoutEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  if (!k) return null;

  const finish = (save: boolean) => {
    if (done.current) return;
    done.current = true;
    ref.current?.blur();
    onClose(save && value !== k.text ? value : null);
  };
  const scale = Math.min(4, Math.max(0.1, k.scale_x || 1));

  return (
    <div
      className="cv-text-edit"
      style={{
        left: pane.x + p.x * z,
        top: pane.y + p.y * z,
        width: p.w * z,
        height: p.h * z,
        transform: p.deg ? `rotate(${p.deg}deg)` : undefined,
      }}
      onPointerDown={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
    >
      <textarea
        {...noAutofill}
        ref={ref}
        value={value}
        readOnly={pending}
        spellCheck={false}
        aria-label="글자 고치기"
        onChange={(e) => setValue(e.target.value)}
        onBlur={() => finish(true)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            finish(false);
          } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            finish(true);
          }
        }}
        style={{
          width: (p.w * z) / scale,
          transform: scale !== 1 ? `scaleX(${scale})` : undefined,
          fontFamily: `"${k.font_family}", sans-serif`,
          fontWeight: k.font_weight,
          fontSize: k.font_size_mm * z,
          lineHeight: k.line_height,
          letterSpacing: k.letter_spacing_mm * z,
          textAlign: k.align,
          writingMode: k.vertical ? "vertical-rl" : undefined,
          whiteSpace: k.vertical ? "pre" : undefined,
          color: outline ? "transparent" : colorOf(layer),
          WebkitTextStroke: outline ? "1px #000" : undefined,
        }}
      />
    </div>
  );
}

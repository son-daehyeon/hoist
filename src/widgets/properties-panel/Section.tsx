import { useState, type ReactNode } from "react";
import { pickScreenColor } from "@/features/eyedropper";
import type { Paint } from "@/shared/api";
import { solid } from "@/shared/lib";
import { Icon } from "@/shared/ui";
import { ColorPicker, ColorSwatch } from "./ColorPicker";

const CLOSED_KEY = "hoist.props-closed";

function closedSections(): Set<string> {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(CLOSED_KEY) ?? "[]");
    return new Set(Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
  } catch {
    return new Set();
  }
}

export function PropSection({
  id,
  title,
  action,
  children,
}: {
  id: string;
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => !closedSections().has(id));
  const toggle = () => {
    const closed = closedSections();
    if (open) closed.add(id);
    else closed.delete(id);
    localStorage.setItem(CLOSED_KEY, JSON.stringify([...closed]));
    setOpen(!open);
  };
  return (
    <section className="pp-sec">
      <div className="pp-sec-head">
        <button type="button" className="pp-sec-toggle" aria-expanded={open} onClick={toggle}>
          <Icon name={open ? "chevron-down" : "chevron-right"} size={12} />
          <span className="t-label-m">{title}</span>
        </button>
        {action}
      </div>
      {open && <div className="pp-sec-body">{children}</div>}
    </section>
  );
}

export function PaintField({
  paint,
  label,
  onChange,
  palette,
  mixed = false,
  allowGradient = true,
}: {
  paint: Paint | null;
  label: string;
  onChange: (p: Paint) => void;
  palette: string[];
  mixed?: boolean;
  allowGradient?: boolean;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <>
      <ColorSwatch paint={paint} mixed={mixed} label={label} onOpen={(el) => setAnchor(anchor ? null : el)} />
      {anchor && (
        <ColorPicker
          anchor={anchor}
          value={paint ?? solid("#FFFFFF")}
          onChange={onChange}
          onClose={() => setAnchor(null)}
          title={label}
          allowGradient={allowGradient}
          palette={palette}
          eyedropper={pickScreenColor}
        />
      )}
    </>
  );
}

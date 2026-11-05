import { useEffect, useRef, useState } from "react";
import { Icon, Popover, SearchField } from "@/shared/ui";
import { ensureFont, fontLabel, groupFonts, type Font } from "@/entities/font";

function FontItem({ font, selected, onPick }: { font: Font; selected: boolean; onPick: () => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      ensureFont(font);
      io.disconnect();
    });
    io.observe(el);
    return () => io.disconnect();
  }, [font]);
  return (
    <button
      ref={ref}
      type="button"
      role="option"
      aria-selected={selected}
      className="pp-font-item"
      title={font.family}
      onClick={onPick}
    >
      <span className="pp-font-check">{selected && <Icon name="check" size={14} />}</span>
      <span style={{ fontFamily: `"${font.family}", var(--font-sans)` }}>{fontLabel(font)}</span>
    </button>
  );
}

export function FontPicker({
  family,
  fonts,
  onPick,
}: {
  family: string;
  fonts: Font[] | null;
  onPick: (family: string) => void;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const [query, setQuery] = useState("");
  const current = fonts?.find((f) => f.family === family);
  const q = query.trim().toLowerCase();
  const shown = (fonts ?? []).filter(
    (f) => !q || fontLabel(f).toLowerCase().includes(q) || f.family.toLowerCase().includes(q),
  );
  const close = () => {
    setAnchor(null);
    setQuery("");
  };

  return (
    <>
      <button
        type="button"
        className="pp-font t-body-m"
        aria-haspopup="listbox"
        title={family}
        onClick={(e) => {
          if (current) ensureFont(current);
          if (anchor) close();
          else setAnchor(e.currentTarget);
        }}
      >
        <span style={{ fontFamily: `"${family}", var(--font-sans)` }}>
          {current ? fontLabel(current) : family}
        </span>
        <Icon name="chevron-down" size={14} />
      </button>
      {anchor && (
        <Popover anchor={anchor} onClose={close} side="right" width={300} label="글꼴">
          <div className="pp-font-pop">
            <SearchField
              width={276}
              autoFocus
              placeholder="글꼴 찾기"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <div className="pp-font-list" role="listbox" aria-label="글꼴">
              {fonts === null && <div className="t-body-m c-tertiary pp-font-note">글꼴을 불러오는 중…</div>}
              {fonts !== null && shown.length === 0 && (
                <div className="t-body-m c-tertiary pp-font-note">찾는 글꼴이 없어요</div>
              )}
              {groupFonts(shown).map((g) => (
                <div key={g.title} role="group" aria-label={g.title}>
                  <div className="pp-font-group t-label-m">{g.title}</div>
                  {g.fonts.map((f) => (
                    <FontItem
                      key={f.family}
                      font={f}
                      selected={f.family === family}
                      onPick={() => {
                        close();
                        if (f.family !== family) onPick(f.family);
                      }}
                    />
                  ))}
                </div>
              ))}
            </div>
          </div>
        </Popover>
      )}
    </>
  );
}

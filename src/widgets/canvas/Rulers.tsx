import { rulerStep, type Panes } from "./viewport";

function ticks(length: number, zoom: number, offset: number, limit: number) {
  const { minor } = rulerStep(zoom);
  const out: { i: number; pos: number; major: boolean; mm: number }[] = [];
  for (let i = 0; i * minor <= length; i++) {
    const pos = offset + i * minor * zoom;
    if (pos >= -40 && pos <= limit) out.push({ i, pos, major: i % 5 === 0, mm: Math.round(i * minor) });
  }
  return out;
}

export function Rulers({
  panes,
  zoom,
  W,
  H,
  size,
}: {
  panes: Panes;
  zoom: number;
  W: number;
  H: number;
  size: { w: number; h: number };
}) {
  const vertical = [panes.original, panes.work].filter((r) => r !== null);
  return (
    <>
      <div className="cv-corner" />
      <div className="cv-ruler cv-ruler-x">
        {ticks(W, zoom, panes.work.x, size.w).map((t) => (
          <span key={t.i} className={`cv-tick ${t.major ? "is-major" : ""}`} style={{ left: t.pos }}>
            {t.major && <span className="t-micro">{t.mm}</span>}
          </span>
        ))}
      </div>
      <div className="cv-ruler cv-ruler-y">
        {vertical.map((r, n) =>
          ticks(H, zoom, r.y, size.h).map((t) => (
            <span
              key={`${n}-${t.i}`}
              className={`cv-tick ${t.major ? "is-major" : ""}`}
              style={{ top: t.pos }}
            >
              {t.major && <span className="t-micro">{t.mm}</span>}
            </span>
          )),
        )}
      </div>
    </>
  );
}

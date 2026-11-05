import { serializePath, type Path, type Pt } from "@/entities/path";
import { centerOf, rotatePoint, type Params } from "./drag";
import type { Rect } from "./viewport";

export type PathSel = [number, number] | null;

export function PathView({
  path,
  f,
  z,
  pane,
  sel,
  fill,
  rubber,
}: {
  path: Path;
  f: Params;
  z: number;
  pane: Rect;
  sel: PathSel;
  fill: string | null;
  rubber?: Pt | null;
}) {
  const screen = (q: Pt) => {
    const w = rotatePoint({ x: f.x + q.x, y: f.y + q.y }, centerOf(f), f.deg);
    return { left: pane.x + w.x * z, top: pane.y + w.y * z };
  };
  const anchor = sel ? path.subpaths[sel[0]]?.anchors[sel[1]] : undefined;
  const handles = anchor
    ? (["in", "out"] as const)
        .map((side) => ({ side, q: side === "in" ? anchor.hIn : anchor.hOut }))
        .filter((h): h is { side: "in" | "out"; q: Pt } => !!h.q)
    : [];
  const last = path.subpaths.at(-1)?.anchors.at(-1)?.p;

  return (
    <>
      <svg className="cv-pathview" style={{ left: pane.x, top: pane.y }} width={1} height={1}>
        <g transform={`scale(${z}) translate(${f.x} ${f.y}) rotate(${f.deg} ${f.w / 2} ${f.h / 2})`}>
          <path d={serializePath(path)} style={{ fill: fill ?? "none" }} />
          {rubber && last && (
            <line className="is-rubber" x1={last.x} y1={last.y} x2={rubber.x} y2={rubber.y} />
          )}
          {anchor &&
            handles.map((h) => <line key={h.side} x1={anchor.p.x} y1={anchor.p.y} x2={h.q.x} y2={h.q.y} />)}
        </g>
      </svg>
      {path.subpaths.flatMap((s, si) =>
        s.anchors.map((a, ai) => {
          const at = screen(a.p);
          const on = sel?.[0] === si && sel[1] === ai;
          return (
            <div
              key={`${si}:${ai}`}
              data-anchor={`${si}:${ai}`}
              className={`cv-anchor ${on ? "is-selected" : ""} ${a.kind === "smooth" ? "is-smooth" : ""}`}
              style={{ left: at.left - 4, top: at.top - 4 }}
            />
          );
        }),
      )}
      {sel &&
        handles.map((h) => {
          const at = screen(h.q);
          return (
            <div
              key={h.side}
              data-hpt={`${sel[0]}:${sel[1]}:${h.side}`}
              className="cv-hpt"
              style={{ left: at.left - 4, top: at.top - 4 }}
            />
          );
        })}
    </>
  );
}

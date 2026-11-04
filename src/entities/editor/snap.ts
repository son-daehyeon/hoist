export type Rect = { x: number; y: number; w: number; h: number };
export type SnapLines = { xs: number[]; ys: number[] };
export type Guide = { axis: "x" | "y"; at: number };

const xMarks = (r: Rect) => [r.x, r.x + r.w / 2, r.x + r.w];
const yMarks = (r: Rect) => [r.y, r.y + r.h / 2, r.y + r.h];

export function snapLines(width: number, height: number, others: Rect[]): SnapLines {
  return {
    xs: [0, width / 2, width, ...others.flatMap(xMarks)],
    ys: [0, height / 2, height, ...others.flatMap(yMarks)],
  };
}

function pull(marks: number[], lines: number[], tolerance: number) {
  let best: number | null = null;
  for (const m of marks) {
    for (const l of lines) {
      const d = l - m;
      if (Math.abs(d) <= tolerance && (best === null || Math.abs(d) < Math.abs(best))) best = d;
    }
  }
  return best ?? 0;
}

export function snapMove(r: Rect, lines: SnapLines, tolerance: number): Rect {
  return {
    ...r,
    x: r.x + pull(xMarks(r), lines.xs, tolerance),
    y: r.y + pull(yMarks(r), lines.ys, tolerance),
  };
}

export function snapResize(r: Rect, handle: string, lines: SnapLines, tolerance: number): Rect {
  let { x, y, w, h } = r;
  if (handle.includes("w")) {
    const d = pull([x], lines.xs, tolerance);
    x += d;
    w -= d;
  }
  if (handle.includes("e")) w += pull([x + w], lines.xs, tolerance);
  if (handle.includes("n")) {
    const d = pull([y], lines.ys, tolerance);
    y += d;
    h -= d;
  }
  if (handle.includes("s")) h += pull([y + h], lines.ys, tolerance);
  return { x, y, w: Math.max(1, w), h: Math.max(1, h) };
}

export function guidesFor(r: Rect, lines: SnapLines): Guide[] {
  const hits = (marks: number[], ls: number[]) => [
    ...new Set(ls.filter((l) => marks.some((m) => Math.abs(m - l) < 0.01))),
  ];
  return [
    ...hits(xMarks(r), lines.xs).map((at) => ({ axis: "x" as const, at })),
    ...hits(yMarks(r), lines.ys).map((at) => ({ axis: "y" as const, at })),
  ];
}

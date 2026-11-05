import type { Path, Pt, Subpath } from "./model.ts";

const num = (n: number): string => String(Number(n.toFixed(2)));
const pt = (p: Pt): string => `${num(p.x)} ${num(p.y)}`;

function subpath(s: Subpath): string {
  const a = s.anchors;
  if (a.length === 0) return "";
  const parts = [`M${pt(a[0].p)}`];
  const n = s.closed ? a.length : a.length - 1;
  for (let i = 0; i < n; i++) {
    const from = a[i];
    const to = a[(i + 1) % a.length];
    if (from.hOut || to.hIn) parts.push(`C${pt(from.hOut ?? from.p)} ${pt(to.hIn ?? to.p)} ${pt(to.p)}`);
    else if (i < a.length - 1) parts.push(`L${pt(to.p)}`);
  }
  if (s.closed) parts.push("Z");
  return parts.join(" ");
}

export const serializePath = (path: Path): string => path.subpaths.map(subpath).filter(Boolean).join(" ");

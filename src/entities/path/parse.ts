import type { Anchor, Path, Pt, Subpath } from "./model.ts";
import { add, closeSub, corner, kindOf, mul, sub } from "./segment.ts";

const ARGS: Record<string, number> = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4, T: 2, Z: 0 };
const TOKEN = /[\s,]*(?:([A-Za-z])|([-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?))/y;

function tokenize(d: string): (string | number)[] {
  const src = d.trim();
  const out: (string | number)[] = [];
  TOKEN.lastIndex = 0;
  while (TOKEN.lastIndex < src.length) {
    const at = TOKEN.lastIndex;
    const m = TOKEN.exec(src);
    if (!m) throw new Error(`path: 해석할 수 없는 문자 "${src.slice(at, at + 8)}"`);
    out.push(m[1] ?? Number(m[2]));
  }
  return out;
}

const reflect = (h: Pt, about: Pt): Pt => sub(mul(about, 2), h);
const finish = (s: Subpath): Subpath => ({
  ...s,
  anchors: s.anchors.map((a) => ({ ...a, kind: kindOf(a) })),
});

export function parsePath(d: string): Path {
  const tokens = tokenize(d);
  const subpaths: Subpath[] = [];
  let anchors: Anchor[] | null = null;
  let cur: Pt = { x: 0, y: 0 };
  let start = cur;
  let lastC2: Pt | null = null;
  let lastQ: Pt | null = null;

  const flush = (closed: boolean) => {
    if (anchors) {
      const s = { closed, anchors };
      subpaths.push(finish(closed ? closeSub(s) : s));
    }
    anchors = null;
  };
  const open = (): Anchor[] => {
    if (!anchors) {
      anchors = [corner(cur)];
      start = cur;
    }
    return anchors;
  };
  const lineTo = (p: Pt) => {
    open().push(corner(p));
    cur = p;
  };
  const curveTo = (c1: Pt, c2: Pt, p: Pt) => {
    const list = open();
    list[list.length - 1].hOut = c1;
    list.push({ p, hIn: c2, hOut: null, kind: "corner" });
    cur = p;
    lastC2 = c2;
  };
  const quadTo = (q: Pt, p: Pt) => {
    curveTo(add(cur, mul(sub(q, cur), 2 / 3)), add(p, mul(sub(q, p), 2 / 3)), p);
    lastQ = q;
  };

  let i = 0;
  let cmd = "";
  while (i < tokens.length) {
    const t = tokens[i];
    if (typeof t === "string") {
      if (!(t.toUpperCase() in ARGS)) throw new Error(`path: 지원하지 않는 명령 ${t}`);
      cmd = t;
      i += 1;
      if (cmd.toUpperCase() === "Z") {
        flush(true);
        cur = start;
        lastC2 = lastQ = null;
        cmd = "";
        continue;
      }
    } else if (!cmd) throw new Error("path: 명령 없이 숫자가 왔다");
    const upper = cmd.toUpperCase();
    const n = ARGS[upper];
    if (i + n > tokens.length || tokens.slice(i, i + n).some((v) => typeof v !== "number"))
      throw new Error(`path: ${cmd} 인자 부족`);
    const v = tokens.slice(i, i + n) as number[];
    i += n;
    const rel = cmd !== upper;
    const at = (x: number, y: number): Pt => (rel ? { x: cur.x + x, y: cur.y + y } : { x, y });
    const [c2, q] = [lastC2, lastQ];
    lastC2 = lastQ = null;
    switch (upper) {
      case "M":
        flush(false);
        cur = at(v[0], v[1]);
        open();
        cmd = rel ? "l" : "L";
        break;
      case "L":
        lineTo(at(v[0], v[1]));
        break;
      case "H":
        lineTo({ x: rel ? cur.x + v[0] : v[0], y: cur.y });
        break;
      case "V":
        lineTo({ x: cur.x, y: rel ? cur.y + v[0] : v[0] });
        break;
      case "C":
        curveTo(at(v[0], v[1]), at(v[2], v[3]), at(v[4], v[5]));
        break;
      case "S":
        curveTo(c2 ? reflect(c2, cur) : cur, at(v[0], v[1]), at(v[2], v[3]));
        break;
      case "Q":
        quadTo(at(v[0], v[1]), at(v[2], v[3]));
        break;
      case "T":
        quadTo(q ? reflect(q, cur) : cur, at(v[0], v[1]));
        break;
    }
  }
  flush(false);
  return { subpaths };
}

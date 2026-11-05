import { useLayoutEffect, useRef, type RefObject } from "react";
import { matrixFor } from "./drag";
import type { Edits } from "./selection";

type Touched = {
  orig: string | null;
  toLocal: DOMMatrix;
  toWorld: DOMMatrix;
  own: DOMMatrix;
  ghost: Element | null;
};

const asMatrix = (m: DOMMatrixInit | null | undefined) => (m ? DOMMatrix.fromMatrix(m) : new DOMMatrix());

function leaveGhost(el: Element) {
  const ghost = el.cloneNode(true) as Element;
  for (const n of [ghost, ...ghost.querySelectorAll("[data-layer]")]) n.removeAttribute("data-layer");
  el.before(ghost);
  return ghost;
}

export function useLivePreview(
  svgRef: RefObject<SVGSVGElement | null>,
  worldRef: RefObject<SVGGElement | null>,
  live: Edits | null,
) {
  const touched = useRef(new Map<SVGGraphicsElement, Touched>());

  useLayoutEffect(() => {
    const svg = svgRef.current;
    const world = worldRef.current;
    const seen = new Set<SVGGraphicsElement>();
    if (svg && world && live) {
      const fromScreen = asMatrix(world.getCTM()).inverse();
      for (const [id, next] of Object.entries(live.next)) {
        const base = live.base[id];
        if (!base) continue;
        const m = new DOMMatrix(matrixFor(base, next));
        for (const el of svg.querySelectorAll<SVGGraphicsElement>(`[data-layer="${CSS.escape(id)}"]`)) {
          let t = touched.current.get(el);
          if (!t) {
            const parentCtm =
              el.parentNode instanceof SVGGraphicsElement ? asMatrix(el.parentNode.getCTM()) : null;
            const toWorld = parentCtm ? fromScreen.multiply(parentCtm) : new DOMMatrix();
            const own = parentCtm ? parentCtm.inverse().multiply(asMatrix(el.getCTM())) : new DOMMatrix();
            const ghost = live.copy ? leaveGhost(el) : null;
            t = { orig: el.getAttribute("transform"), toLocal: toWorld.inverse(), toWorld, own, ghost };
            touched.current.set(el, t);
          }
          const r = t.toLocal.multiply(m).multiply(t.toWorld).multiply(t.own);
          el.setAttribute("transform", `matrix(${r.a} ${r.b} ${r.c} ${r.d} ${r.e} ${r.f})`);
          seen.add(el);
        }
      }
    }
    for (const [el, t] of touched.current) {
      if (seen.has(el)) continue;
      if (t.orig === null) el.removeAttribute("transform");
      else el.setAttribute("transform", t.orig);
      t.ghost?.remove();
      touched.current.delete(el);
    }
  });
}

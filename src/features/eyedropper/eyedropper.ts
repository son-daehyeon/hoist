import { rgbToHex } from "@/shared/lib";
import "./eyedropper.css";

const inside = (r: DOMRect, x: number, y: number) => x >= r.left && x < r.right && y >= r.top && y < r.bottom;

function clippedAway(el: Element, x: number, y: number) {
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (getComputedStyle(p).overflow !== "visible" && !inside(p.getBoundingClientRect(), x, y)) return true;
  }
  return false;
}

function sourcePixel(img: HTMLImageElement, x: number, y: number): [number, number] | null {
  const r = img.getBoundingClientRect();
  const nw = img.naturalWidth;
  const nh = img.naturalHeight;
  if (!nw || !nh || !inside(r, x, y)) return null;
  let box = { left: r.left, top: r.top, width: r.width, height: r.height };
  const fit = getComputedStyle(img).objectFit;
  if (fit === "contain" || fit === "cover") {
    const s = (fit === "contain" ? Math.min : Math.max)(r.width / nw, r.height / nh);
    box = {
      left: r.left + (r.width - nw * s) / 2,
      top: r.top + (r.height - nh * s) / 2,
      width: nw * s,
      height: nh * s,
    };
  }
  const sx = Math.floor(((x - box.left) / box.width) * nw);
  const sy = Math.floor(((y - box.top) / box.height) * nh);
  return sx >= 0 && sy >= 0 && sx < nw && sy < nh ? [sx, sy] : null;
}

function cssBackground(el: Element | null) {
  for (let p = el; p; p = p.parentElement) {
    const c = getComputedStyle(p)
      .backgroundColor.match(/[\d.]+/g)
      ?.map(Number);
    if (c && (c[3] ?? 1) > 0) return rgbToHex({ r: c[0], g: c[1], b: c[2] });
  }
  return "#FFFFFF";
}

async function svgImage(svg: SVGSVGElement, r: DOMRect) {
  const clone = svg.cloneNode(true);
  if (!(clone instanceof SVGSVGElement)) return null;
  clone.setAttribute("xmlns", "http://www.w3.org/2000/svg");
  clone.setAttribute("width", String(r.width));
  clone.setAttribute("height", String(r.height));
  const img = new Image();
  img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(new XMLSerializer().serializeToString(clone))}`;
  await img.decode();
  return img;
}

async function colorAt(x: number, y: number): Promise<string> {
  const hit = document.elementFromPoint(x, y);
  if (!hit) return "#FFFFFF";
  const media = [...document.querySelectorAll("img, svg")].filter(
    (el): el is HTMLImageElement | SVGSVGElement =>
      (el instanceof HTMLImageElement ||
        (el instanceof SVGSVGElement && !el.parentElement?.closest("svg"))) &&
      (el === hit || el.contains(hit) || hit.contains(el)) &&
      el.checkVisibility({ opacityProperty: true }) &&
      inside(el.getBoundingClientRect(), x, y) &&
      !clippedAway(el, x, y),
  );
  const base = cssBackground(media[0]?.parentElement ?? hit);
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return base;
  ctx.fillStyle = base;
  ctx.fillRect(0, 0, 1, 1);
  for (const el of media) {
    ctx.globalAlpha = Number(getComputedStyle(el).opacity) || 1;
    if (el instanceof HTMLImageElement) {
      const at = sourcePixel(el, x, y);
      if (at) ctx.drawImage(el, at[0], at[1], 1, 1, 0, 0, 1, 1);
    } else {
      const r = el.getBoundingClientRect();
      const img = await svgImage(el, r).catch(() => null);
      if (img) ctx.drawImage(img, x - r.left, y - r.top, 1, 1, 0, 0, 1, 1);
    }
  }
  try {
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return rgbToHex({ r, g, b });
  } catch {
    // A cross-origin image taints the canvas; fall back to the surface color.
    return base;
  }
}

export function pickScreenColor(): Promise<string | null> {
  return new Promise((resolve) => {
    const root = document.documentElement;
    const swallow = (e: Event) => {
      e.preventDefault();
      e.stopPropagation();
    };
    const release = () => {
      window.removeEventListener("pointerup", swallow, true);
      window.removeEventListener("click", swallowClick, true);
    };
    const swallowClick = (e: Event) => {
      swallow(e);
      release();
    };
    const finish = (waitForClick: boolean) => {
      root.classList.remove("is-eyedropping");
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("keydown", key, true);
      if (waitForClick) window.setTimeout(release, 1500);
      else release();
    };
    const down = (e: PointerEvent) => {
      swallow(e);
      const color = e.button === 0 ? colorAt(e.clientX, e.clientY) : Promise.resolve(null);
      finish(true);
      color.then(resolve, () => resolve(null));
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      swallow(e);
      finish(false);
      resolve(null);
    };
    root.classList.add("is-eyedropping");
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("pointerup", swallow, true);
    window.addEventListener("click", swallowClick, true);
    window.addEventListener("keydown", key, true);
  });
}

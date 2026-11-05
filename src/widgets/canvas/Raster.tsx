import { memo } from "react";

export const Raster = memo(function Raster({
  src,
  left,
  top,
  width,
  height,
  transform,
}: {
  src: string;
  left: number;
  top: number;
  width: number;
  height: number;
  transform?: string;
}) {
  return (
    <img
      className={`cv-raster ${transform ? "is-moving" : ""}`}
      src={src}
      alt=""
      draggable={false}
      style={{ left, top, width, height, transform }}
    />
  );
});

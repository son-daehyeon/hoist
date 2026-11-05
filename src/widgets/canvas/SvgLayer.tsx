import { memo } from "react";

export const SvgLayer = memo(function SvgLayer({ svg }: { svg: string }) {
  return <g dangerouslySetInnerHTML={{ __html: svg }} />;
});

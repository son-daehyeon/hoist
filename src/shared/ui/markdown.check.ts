import { parseMarkdown } from "./markdown.ts";

const same = (a: unknown, b: unknown) => {
  const [x, y] = [JSON.stringify(a), JSON.stringify(b)];
  if (x !== y) throw new Error(`${x} !== ${y}`);
};
const flat = (t: string) => parseMarkdown(t).map((s) => `${s.kind}:${s.text}`);

same(flat("그냥 글"), ["text:그냥 글"]);
same(flat("**제목**을 `fit_box`로 *살짝*"), [
  "bold:제목",
  "text:을 ",
  "code:fit_box",
  "text:로 ",
  "italic:살짝",
]);
same(flat("- 하나\n- **둘**"), ["text:• ", "text:하나", "text:\n", "text:• ", "bold:둘"]);
same(flat("## 결과"), ["bold:결과"]);
same(flat("2 * 3 = 6, 5×3"), ["text:2 * 3 = 6, 5×3"]);
same(flat("<b>태그</b>"), ["text:<b>태그</b>"]);
console.log("markdown: ok");

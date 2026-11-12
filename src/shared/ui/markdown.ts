// Line breaks stay as text: the containers render them with white-space: pre-wrap.
type MdSpan = { kind: "text" | "bold" | "italic" | "code"; text: string };

const INLINE = /(\*\*[^*\n]+\*\*|__[^_\n]+__|`[^`\n]+`|\*[^*\s\n][^*\n]*\*)/g;

function inline(text: string, into: MdSpan[], kind: MdSpan["kind"] = "text") {
  let at = 0;
  for (const m of text.matchAll(INLINE)) {
    const i = m.index ?? 0;
    if (i > at) into.push({ kind, text: text.slice(at, i) });
    const tok = m[0];
    if (tok.startsWith("`")) into.push({ kind: "code", text: tok.slice(1, -1) });
    else if (tok.startsWith("**") || tok.startsWith("__"))
      into.push({ kind: "bold", text: tok.slice(2, -2) });
    else into.push({ kind: kind === "bold" ? "bold" : "italic", text: tok.slice(1, -1) });
    at = i + tok.length;
  }
  if (at < text.length) into.push({ kind, text: text.slice(at) });
}

export function parseMarkdown(text: string): MdSpan[] {
  const out: MdSpan[] = [];
  text.split("\n").forEach((line, n) => {
    if (n > 0) out.push({ kind: "text", text: "\n" });
    const heading = /^\s{0,3}#{1,6}\s+(.*)$/.exec(line);
    if (heading) return inline(heading[1].replace(/\*\*/g, ""), out, "bold");
    const bullet = /^(\s*)[-*+]\s+(.*)$/.exec(line);
    if (bullet) {
      out.push({ kind: "text", text: `${bullet[1]}• ` });
      return inline(bullet[2], out);
    }
    inline(line, out);
  });
  return out.filter((s) => s.text !== "");
}

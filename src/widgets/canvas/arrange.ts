export type Arrange = "forward" | "backward" | "front" | "back";

export function reorder(order: string[], picked: ReadonlySet<string>, dir: Arrange) {
  const o = [...order];
  const moves: { id: string; index: number }[] = [];
  const put = (id: string, to: number) => {
    if (o.indexOf(id) === to) return;
    o.splice(o.indexOf(id), 1);
    o.splice(to, 0, id);
    moves.push({ id, index: to });
  };
  const chosen = order.filter((id) => picked.has(id));
  if (dir === "forward")
    for (let i = o.length - 2; i >= 0; i--) {
      if (picked.has(o[i]) && !picked.has(o[i + 1])) put(o[i], i + 1);
    }
  if (dir === "backward")
    for (let i = 1; i < o.length; i++) {
      if (picked.has(o[i]) && !picked.has(o[i - 1])) put(o[i], i - 1);
    }
  if (dir === "front") for (const id of chosen) put(id, o.length - 1);
  if (dir === "back") for (const id of chosen.reverse()) put(id, 0);
  return { order: o, moves };
}

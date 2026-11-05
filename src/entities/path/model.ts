export type Pt = { x: number; y: number };
export type Anchor = { p: Pt; hIn: Pt | null; hOut: Pt | null; kind: "corner" | "smooth" };
export type Subpath = { closed: boolean; anchors: Anchor[] };
export type Path = { subpaths: Subpath[] };
export type Rect = { x: number; y: number; w: number; h: number };

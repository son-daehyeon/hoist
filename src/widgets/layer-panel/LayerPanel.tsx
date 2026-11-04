import { useState, type MouseEvent } from "react";
import { useEd } from "@/entities/editor";
import { defaultRect, layerIcon, rectLayer, STRATEGY_SHORT, textLayer } from "@/entities/layer";
import { childrenOf } from "@/shared/lib";
import type { Document, Layer, LayerId } from "@/shared/api";
import { IconButton, LayerRow, Menu, PanelHeader } from "@/shared/ui";
import "./layer-panel.css";

export function LayerPanel() {
  const { doc, selection, setSelection, ops } = useEd();
  const [collapsed, setCollapsed] = useState<Set<LayerId>>(new Set());
  const [dragId, setDragId] = useState<LayerId | null>(null);
  const [drop, setDrop] = useState<{ id: LayerId; where: "above" | "below" } | null>(null);
  const [addMenu, setAddMenu] = useState<DOMRect | null>(null);
  const [seenDoc, setSeenDoc] = useState<Document | null>(doc);
  const [fresh, setFresh] = useState<ReadonlySet<LayerId>>(new Set());
  if (seenDoc !== doc) {
    setSeenDoc(doc);
    if (seenDoc && doc) {
      const before = new Set(seenDoc.layers.map((l) => l.id));
      setFresh(new Set(doc.layers.filter((l) => !before.has(l.id)).map((l) => l.id)));
    }
  }
  if (!doc) return <aside className="ed-layers" />;
  const layers = doc.layers;

  const rows: { layer: Layer; depth: number }[] = [];
  const walk = (parent: LayerId | null, depth: number) => {
    for (const l of childrenOf(layers, parent).reverse()) {
      rows.push({ layer: l, depth });
      if (l.kind.type === "group" && !collapsed.has(l.id)) walk(l.id, depth + 1);
    }
  };
  walk(null, 0);

  const isInside = (id: LayerId, ancestor: LayerId): boolean => {
    const l = layers.find((x) => x.id === id);
    return !!l?.parent && (l.parent === ancestor || isInside(l.parent, ancestor));
  };

  const select = (e: MouseEvent, id: LayerId) => {
    if (e.shiftKey || e.ctrlKey || e.metaKey)
      setSelection(selection.includes(id) ? selection.filter((s) => s !== id) : [...selection, id]);
    else setSelection([id]);
  };

  const moveTo = (target: Layer, where: "above" | "below") => {
    if (!dragId || dragId === target.id || isInside(target.id, dragId)) return;
    const parent = target.parent ?? null;
    const siblings = childrenOf(layers, parent).filter((l) => l.id !== dragId);
    const ti = siblings.findIndex((l) => l.id === target.id);
    void ops([{ op: "move_layer", id: dragId, parent, index: where === "above" ? ti + 1 : ti }]);
  };

  const add = (layer: Layer) => {
    void ops([{ op: "add_layer", layer }]).then((ok) => ok && setSelection([layer.id]));
  };
  const cx = doc.artboard.width_mm / 2;
  const cy = doc.artboard.height_mm / 2;

  return (
    <aside className="ed-layers">
      <PanelHeader title="레이어">
        <IconButton
          icon="plus"
          label="레이어 추가"
          onClick={(e) => setAddMenu(e.currentTarget.getBoundingClientRect())}
        />
      </PanelHeader>
      <div className="ed-layers-list" onClick={(e) => e.target === e.currentTarget && setSelection([])}>
        {rows.map(({ layer: l, depth }) => (
          <LayerRow
            key={l.id}
            depth={depth}
            name={l.name}
            icon={layerIcon(l)}
            tag={l.kind.type === "group" ? undefined : STRATEGY_SHORT[l.strategy]}
            pinned={l.pinned.length > 0}
            visible={l.visible}
            selected={selection.includes(l.id)}
            fresh={fresh.has(l.id)}
            collapsed={l.kind.type === "group" ? collapsed.has(l.id) : undefined}
            onToggleCollapsed={
              l.kind.type === "group"
                ? () =>
                    setCollapsed((c) => {
                      const n = new Set(c);
                      if (!n.delete(l.id)) n.add(l.id);
                      return n;
                    })
                : undefined
            }
            onSelect={(e) => select(e, l.id)}
            onToggleVisible={() =>
              void ops([{ op: "update_layer", id: l.id, patch: { visible: !l.visible } }])
            }
            onRename={(name) => void ops([{ op: "update_layer", id: l.id, patch: { name } }])}
            drop={drop?.id === l.id ? drop.where : null}
            dragHandlers={{
              draggable: true,
              onDragStart: (e) => {
                e.dataTransfer.effectAllowed = "move";
                setDragId(l.id);
              },
              onDragOver: (e) => {
                if (!dragId) return;
                e.preventDefault();
                const r = e.currentTarget.getBoundingClientRect();
                setDrop({ id: l.id, where: e.clientY < r.top + r.height / 2 ? "above" : "below" });
              },
              onDragLeave: () => setDrop(null),
              onDrop: (e) => {
                e.preventDefault();
                if (drop) moveTo(l, drop.where);
                setDrop(null);
                setDragId(null);
              },
              onDragEnd: () => {
                setDrop(null);
                setDragId(null);
              },
            }}
          />
        ))}
      </div>
      {addMenu && (
        <Menu
          anchor={addMenu}
          align="end"
          onClose={() => setAddMenu(null)}
          items={[
            { label: "글자 추가", icon: "type", onSelect: () => add(textLayer(doc, cx, cy)) },
            {
              label: "사각형 추가",
              icon: "square",
              onSelect: () => add(rectLayer(doc, defaultRect(doc, cx, cy))),
            },
          ]}
        />
      )}
    </aside>
  );
}

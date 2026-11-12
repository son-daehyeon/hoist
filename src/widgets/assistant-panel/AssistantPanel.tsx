import { useLayoutEffect, useRef, useState } from "react";
import { isAutoFirst, useEd } from "@/entities/editor";
import type { Capture } from "@/shared/api";
import { ColumnResizer, IconButton, Menu, Tabs, Tooltip, useColumnWidth } from "@/shared/ui";
import { ChatEntry } from "./ChatEntry";
import { Composer, ContextTag } from "./Composer";
import { History } from "./History";
import "./chat.css";
import "./assistant-panel.css";

// Same rate as openai.rs.
const KRW_PER_USD = 1400;
const usd = (krw: number) => `$${(krw / KRW_PER_USD).toFixed(2)}`;

const NEAR_BOTTOM_PX = 48;

function captureLabel(c: Capture, W: number) {
  const mid = (c.x_mm + c.w_mm / 2) / W;
  const side = mid < 1 / 3 ? "왼쪽" : mid > 2 / 3 ? "오른쪽" : "가운데";
  return `캡처 · ${c.pane === "work" ? "작업본" : "원본"} ${side}`;
}

export function AssistantPanel() {
  const ed = useEd();
  const {
    doc,
    project,
    chat,
    live,
    currentTurn,
    tab,
    setTab,
    selection,
    setSelection,
    captures,
    setCaptures,
    composerLock,
  } = ed;
  const [text, setText] = useState("");
  const [layerMenu, setLayerMenu] = useState<DOMRect | null>(null);
  const panel = useColumnWidth("hoist.assistant-width", { min: 320, max: 900, initial: 440, edge: "left" });
  const listRef = useRef<HTMLDivElement>(null);
  const followNewest = useRef(true);
  const dpi = project?.target_dpi ?? 100;
  const W = doc?.artboard.width_mm ?? 1;

  useLayoutEffect(() => {
    const el = listRef.current;
    if (el && followNewest.current) el.scrollTop = el.scrollHeight;
  }, [chat, live]);
  useLayoutEffect(() => {
    const el = listRef.current;
    followNewest.current = true;
    if (el) el.scrollTop = el.scrollHeight;
  }, [tab]);

  const send = () => {
    const t = text.trim();
    if (!t) return;
    setText("");
    followNewest.current = true;
    void ed.send(t, { layer_ids: selection, captures });
  };

  const layerTags = selection
    .map((id) => doc?.layers.find((l) => l.id === id))
    .filter((l) => l !== undefined)
    .map((l) => (
      <ContextTag
        key={l.id}
        icon="layers"
        label={l.name}
        onRemove={() => setSelection(selection.filter((s) => s !== l.id))}
      />
    ));
  const captureTags = captures.map((c, i) => (
    <ContextTag
      key={`c${i}`}
      icon="crop"
      label={captureLabel(c, W)}
      onRemove={() => setCaptures(captures.filter((_, j) => j !== i))}
    />
  ));
  const tags = [...layerTags, ...captureTags];

  return (
    <aside className="as" style={{ width: panel.width }} data-tour="chat">
      <ColumnResizer column={panel} label="작업 화면과 에이전트 사이 경계" />
      <div className="as-head">
        <Tabs
          value={tab}
          onChange={setTab}
          options={[
            { value: "assistant", label: "에이전트" },
            { value: "history", label: "기록" },
          ]}
        />
        <span className="as-head-spacer" />
        {project && project.usage_krw > 0 && (
          <Tooltip label={`대화 ${usd(project.usage_text_krw)} · 이미지 ${usd(project.usage_image_krw)}`}>
            <span className="as-cost t-micro c-tertiary">{usd(project.usage_krw)}</span>
          </Tooltip>
        )}
      </div>
      {tab === "history" ? (
        <History />
      ) : (
        <>
          <div
            className="as-body"
            ref={listRef}
            onScroll={(e) => {
              const el = e.currentTarget;
              followNewest.current = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX;
            }}
          >
            {chat.map((item, i) =>
              isAutoFirst(item, i) ? null : (
                <ChatEntry
                  key={item.id}
                  item={item}
                  live={live[item.id]}
                  working={currentTurn?.id === item.id}
                  dpi={dpi}
                  onAnswer={(o, q) => ed.answer(item, o, q)}
                />
              ),
            )}
          </div>
          <div className="as-composer" data-tour="composer">
            <Composer
              value={text}
              onChange={setText}
              onSend={send}
              onStop={ed.stop}
              lock={composerLock}
              tags={tags.length ? tags : undefined}
              leading={
                <>
                  <IconButton
                    icon="paperclip"
                    label="영역 캡처해서 붙이기"
                    shortcut="C"
                    disabled={composerLock !== null}
                    onClick={() => ed.setTool("capture")}
                  />
                  <IconButton
                    icon="layers"
                    label="레이어 붙이기"
                    disabled={composerLock !== null}
                    onClick={(e) => setLayerMenu(e.currentTarget.getBoundingClientRect())}
                  />
                </>
              }
            />
          </div>
        </>
      )}
      {layerMenu && doc && (
        <Menu
          anchor={layerMenu}
          onClose={() => setLayerMenu(null)}
          width={220}
          items={doc.layers
            .filter((l) => l.kind.type !== "group")
            .map((l) => ({
              label: l.name,
              checked: selection.includes(l.id),
              onSelect: () =>
                setSelection(
                  selection.includes(l.id) ? selection.filter((s) => s !== l.id) : [...selection, l.id],
                ),
            }))}
        />
      )}
    </aside>
  );
}

import { useLayoutEffect, useRef, useState } from "react";
import { timelineOf, useEd, type LiveTurn } from "@/entities/editor";
import type { Capture, ChatItem, Report } from "@/shared/api";
import { pngSrc, timeLabel } from "@/shared/lib";
import {
  ActionList,
  Button,
  Caret,
  CompareReport,
  Composer,
  ContextTag,
  Dialog,
  EditSummary,
  Icon,
  IconButton,
  Menu,
  Message,
  Question,
  Tabs,
  Thinking,
  VersionRow,
} from "@/shared/ui";
import { usePanelWidth } from "./usePanelWidth";
import "./assistant-panel.css";

const HINT = "다른 부분이 보이면 그 레이어를 고르거나 캡처 도구로 영역을 표시해서 말씀해 주세요.";

function captureLabel(c: Capture, W: number) {
  const mid = (c.x_mm + c.w_mm / 2) / W;
  const side = mid < 1 / 3 ? "왼쪽" : mid > 2 / 3 ? "오른쪽" : "가운데";
  return `캡처 · ${c.pane === "work" ? "작업본" : "원본"} ${side}`;
}

function reportStats(r: Report, dpi: number) {
  const ok = r.min_dpi >= dpi ? `실물 ${dpi}dpi 충족` : `실물 최저 ${r.min_dpi}dpi`;
  return `겹침 ${(r.overlap * 100).toFixed(1)}% · ${ok} · 방식 바꿈 ${r.promoted}`;
}

function ChatEntry({
  item,
  live,
  working,
  dpi,
  onAnswer,
}: {
  item: ChatItem;
  live?: LiveTurn;
  working: boolean;
  dpi: number;
  onAnswer: (o: string) => void;
}) {
  if (item.role === "user") {
    return (
      <>
        <Message role="user">{item.text}</Message>
        {item.edit_summary?.length ? (
          <EditSummary title={`직접 수정 ${item.edit_summary.length}건`} lines={item.edit_summary} />
        ) : null}
      </>
    );
  }
  const timeline = timelineOf(item, live);
  const lastText = timeline.map((x) => x.kind).lastIndexOf("text");
  return (
    <Message role="agent" working={working}>
      {item.question && item.answer && (
        <div className="as-answer t-caption c-tertiary">
          <Icon name="check" size={12} />
          {item.answer}
        </div>
      )}
      {timeline.map((x, i) =>
        x.kind === "thinking" ? (
          <Thinking key={i} started={x.started} seconds={x.seconds} />
        ) : x.kind === "actions" ? (
          <ActionList key={i} items={x.items} />
        ) : x.kind === "text" ? (
          <span key={i}>
            {x.text}
            {i === lastText && live?.typing && <Caret />}
          </span>
        ) : (
          <div key={i} className="t-caption c-tertiary">
            대화를 정리했어요 · {x.reason}
          </div>
        ),
      )}
      {item.report && (
        <CompareReport
          originalSrc={pngSrc(item.report.original_png)}
          workSrc={pngSrc(item.report.work_png)}
          stats={reportStats(item.report, dpi)}
        />
      )}
      {item.question && !item.answer && (
        <Question text={item.question.text} options={item.question.options} onAnswer={onAnswer} />
      )}
    </Message>
  );
}

function History() {
  const { versions, restore, busy } = useEd();
  const list = [...versions].reverse();
  const [picked, setPicked] = useState<number | null>(null);
  const current = list[0]?.seq;
  const sel = picked ?? current;
  return (
    <>
      <div className="as-body as-history">
        {list.map((v) => (
          <VersionRow
            key={v.seq}
            time={v.seq === current ? "지금" : timeLabel(v.ts)}
            summary={`${v.summary} · ${v.actor === "agent" ? "에이전트" : "나"}`}
            thumbnail={pngSrc(v.thumbnail_png)}
            current={v.seq === sel}
            onClick={() => setPicked(v.seq)}
          />
        ))}
      </div>
      <div className="as-history-foot">
        <Button
          icon="undo"
          className="as-wide"
          disabled={busy || sel === undefined || sel === current}
          onClick={() => {
            if (sel === undefined) return;
            restore(sel);
            setPicked(null);
          }}
        >
          이 버전으로 되돌리기
        </Button>
        <span className="t-caption c-tertiary">되돌려도 지금 상태는 기록에 남아요</span>
      </div>
    </>
  );
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
    pendingQuestion,
    busy,
  } = ed;
  const [text, setText] = useState("");
  const [layerMenu, setLayerMenu] = useState<DOMRect | null>(null);
  const [moreMenu, setMoreMenu] = useState<DOMRect | null>(null);
  const [showMemory, setShowMemory] = useState(false);
  const panel = usePanelWidth();
  const listRef = useRef<HTMLDivElement>(null);
  const dpi = project?.target_dpi ?? 100;
  const W = doc?.artboard.width_mm ?? 1;

  useLayoutEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [chat, live, tab]);

  const send = () => {
    const t = text.trim();
    if (!t) return;
    setText("");
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
    <aside className="as" style={{ width: panel.width }}>
      <div className={`as-resize ${panel.resizing ? "is-active" : ""}`} {...panel.handle} />
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
          <span className="as-cost t-micro c-tertiary">
            ₩{Math.round(project.usage_krw).toLocaleString("ko-KR")}
          </span>
        )}
        <IconButton
          icon="more"
          label="더 보기"
          onClick={(e) => setMoreMenu(e.currentTarget.getBoundingClientRect())}
        />
      </div>
      {tab === "history" ? (
        <History />
      ) : (
        <>
          <div className="as-body" ref={listRef}>
            {chat.map((item) => (
              <ChatEntry
                key={item.id}
                item={item}
                live={live[item.id]}
                working={currentTurn?.id === item.id}
                dpi={dpi}
                onAnswer={(o) => ed.answer(item, o)}
              />
            ))}
            {!busy && !pendingQuestion && <Message role="agent">{HINT}</Message>}
          </div>
          <div className="as-composer">
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
      {moreMenu && (
        <Menu
          anchor={moreMenu}
          align="end"
          onClose={() => setMoreMenu(null)}
          items={[
            { label: "에이전트가 기억하는 내용", icon: "book-open", onSelect: () => setShowMemory(true) },
          ]}
        />
      )}
      {showMemory && (
        <Dialog
          title="에이전트가 기억하는 내용"
          onClose={() => setShowMemory(false)}
          footer={
            <Button kind="primary" onClick={() => setShowMemory(false)}>
              확인
            </Button>
          }
        >
          <div className="as-memory t-body-m">{ed.memory.trim() || "아직 기억하는 내용이 없어요."}</div>
        </Dialog>
      )}
    </aside>
  );
}

import { useState, type ReactNode } from "react";
import { openQuestion, summarize, timelineOf, type LiveTurn, type TimelineEntry } from "@/entities/editor";
import { AUTO_ANSWER, type ChatItem, type Question, type Report } from "@/shared/api";
import { dpiVerdict, pngSrc } from "@/shared/lib";
import { Button, Icon, Md } from "@/shared/ui";
import { ActionList } from "./ActionList";
import { TurnSummary } from "./TurnSummary";

const answerLabel = (q: Question, a: string) =>
  a === AUTO_ANSWER ? (q.auto_label ?? "에이전트가 알아서") : a;

const inRound = (x: TimelineEntry, round: number | null) =>
  (x.kind === "thinking" || x.kind === "actions") && x.round === round;

const reportStats = (r: Report, dpi: number) =>
  `겹침 ${(r.overlap * 100).toFixed(1)}% · ${dpiVerdict(r, dpi)}`;

function Message({
  role,
  working = false,
  children,
}: {
  role: "user" | "agent";
  working?: boolean;
  children: ReactNode;
}) {
  if (role === "user") return <div className="bs-msg-user t-body-m">{children}</div>;
  return (
    <div className="bs-msg-agent">
      <span className={`bs-msg-avatar ${working ? "is-working" : ""}`}>
        <Icon name="sparkles" size={12} />
      </span>
      <div className="bs-msg-body t-body-m">{children}</div>
    </div>
  );
}

function Caret() {
  return <span className="bs-caret" aria-hidden />;
}

function Thinking() {
  return <div className="bs-thinking t-caption">생각하는 중</div>;
}

function CompareReport({
  originalSrc,
  workSrc,
  stats,
}: {
  originalSrc: string;
  workSrc: string;
  stats: string;
}) {
  return (
    <div className="bs-report">
      <div className="bs-report-thumbs">
        <figure>
          <figcaption className="t-micro c-tertiary">원본</figcaption>
          <img src={originalSrc} alt="원본" />
        </figure>
        <figure>
          <figcaption className="t-micro c-tertiary">작업본</figcaption>
          <img src={workSrc} alt="작업본" />
        </figure>
      </div>
      <div className="t-caption c-secondary">{stats}</div>
    </div>
  );
}

function QuestionCard({
  text,
  options,
  onAnswer,
}: {
  text: string;
  options: string[];
  onAnswer: (option: string) => void;
}) {
  return (
    <div className="bs-question">
      <div className="t-body-m">
        <Md text={text} />
      </div>
      {options.map((o, i) => (
        <Button key={o} kind={i === 0 ? "primary" : "secondary"} onClick={() => onAnswer(o)}>
          {o}
        </Button>
      ))}
    </div>
  );
}

function EditSummary({ title, lines }: { title: string; lines: string[] }) {
  return (
    <div className="bs-edit-summary">
      <div className="t-label-s">{title}</div>
      {lines.map((l, i) => (
        <div key={i} className="t-caption c-secondary">
          · {l}
        </div>
      ))}
    </div>
  );
}

export function ChatEntry({
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
  onAnswer: (o: string, q: Question) => void;
}) {
  const [open, setOpen] = useState(true);
  if (item.role === "user") {
    return (
      <>
        <Message role="user">{item.text}</Message>
        {item.edit_summary?.length ? <EditSummary title="직접 수정" lines={item.edit_summary} /> : null}
      </>
    );
  }
  const timeline = timelineOf(item, live);
  const lastText = timeline.map((x) => x.kind).lastIndexOf("text");
  const sum = summarize(timeline);
  const asked = item.questions ?? (item.question ? [item.question] : []);
  const given = item.answers ?? (item.answer !== undefined ? [item.answer] : []);
  const answered = given.flatMap((a, i) => (a != null && asked[i] ? [answerLabel(asked[i], a)] : []));
  const waiting = working ? asked[openQuestion(item)] : undefined;
  const streaming = timeline.at(-1)?.kind === "text" ? timeline.length - 1 : -1;
  const visible = (x: TimelineEntry, i: number) =>
    !sum || open || (working ? inRound(x, sum.round) || i === streaming : i === lastText);
  return (
    <Message role="agent" working={working}>
      {answered.length > 0 && (
        <div className="as-answer t-caption c-tertiary">
          <Icon name="check" size={12} />
          <span>{answered.join(", ")}</span>
        </div>
      )}
      {sum && (
        <TurnSummary
          parts={sum.count ? [`작업 ${sum.count}개`] : []}
          thinkingSec={live ? sum.thinkingSec : (item.thinking_sec ?? 0)}
          thinkingSince={sum.thinkingSince}
          failed={sum.failed}
          stopped={sum.stopped}
          expanded={open}
          onToggle={() => setOpen((v) => !v)}
        />
      )}
      {timeline.map((x, i) =>
        !visible(x, i) ? null : x.kind === "thinking" ? (
          x.seconds === null && <Thinking key={i} />
        ) : x.kind === "actions" ? (
          <ActionList key={i} items={x.items} />
        ) : x.kind === "text" ? (
          <span key={i}>
            <Md text={x.text} />
            {i === lastText && live?.typing && <Caret />}
          </span>
        ) : (
          <div key={i} className="t-caption c-tertiary">
            대화를 정리했어요
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
      {waiting && (
        <QuestionCard text={waiting.text} options={waiting.options} onAnswer={(o) => onAnswer(o, waiting)} />
      )}
    </Message>
  );
}

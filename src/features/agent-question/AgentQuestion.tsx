import { openQuestion, useEd } from "@/entities/editor";
import { AUTO_ANSWER, type Question } from "@/shared/api";
import { pngSrc } from "@/shared/lib";
import { QuestionDialog } from "@/shared/ui";

type AskQuestion = Question & { allow_free_text?: boolean; image_png?: string | null; auto_label?: string };

export function AgentQuestion() {
  const { pendingQuestion: item, answer } = useEd();
  if (!item) return null;
  const questions: AskQuestion[] = item.questions?.length
    ? item.questions
    : item.question
      ? [item.question]
      : [];
  const index = openQuestion(item);
  const q = questions[index];
  if (!q) return null;
  return (
    <QuestionDialog
      key={q.id}
      step={index + 1}
      total={questions.length}
      text={q.text}
      image={pngSrc(q.image_png)}
      options={q.options}
      allowFreeText={q.allow_free_text ?? false}
      autoLabel={q.auto_label ?? ""}
      onAnswer={(a) => answer(item, a, q)}
      onAuto={() => answer(item, AUTO_ANSWER, q)}
    />
  );
}

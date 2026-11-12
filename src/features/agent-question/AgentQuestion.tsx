import { openQuestion, questionsOf, useEd } from "@/entities/editor";
import { AUTO_ANSWER } from "@/shared/api";
import { pngSrc } from "@/shared/lib";
import { QuestionDialog } from "./QuestionDialog";

export function AgentQuestion() {
  const { pendingQuestion: item, answer } = useEd();
  if (!item) return null;
  const questions = questionsOf(item);
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

import { Button, Icon } from "@/shared/ui";
import "./onboarding.css";

export function StartCard({ onStart }: { onStart: () => void }) {
  return (
    <div className="ob-start" data-tour="start">
      <Icon name="hoist" size={28} />
      <Button kind="primary" size="l" onClick={onStart}>
        시작하기
      </Button>
    </div>
  );
}

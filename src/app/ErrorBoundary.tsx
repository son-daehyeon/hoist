import { Component, type ReactNode } from "react";
import { Button, EmptyState } from "@/shared/ui";
import "./App.css";

export class ErrorBoundary extends Component<
  { onReset: () => void; children: ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="app-error">
        <EmptyState
          icon="alert"
          title="문제가 생겼어요"
          description="잠시 뒤 다시 해 보세요. 작업한 내용은 저장되어 있어요."
          action={
            <Button
              kind="primary"
              onClick={() => {
                this.setState({ error: null });
                this.props.onReset();
              }}
            >
              처음 화면으로
            </Button>
          }
        />
      </div>
    );
  }
}

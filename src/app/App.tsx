import { Component, useEffect, useState, type ReactNode } from "react";
import { errorMessage } from "@/shared/api";
import { Editor } from "@/pages/editor";
import { Home } from "@/pages/home";
import { Button, EmptyState, toast, ToastHost } from "@/shared/ui";
import "./App.css";

export type Route =
  { page: "home"; newProject?: boolean } | { page: "editor"; id: string; firstMessage?: string };

class ErrorBoundary extends Component<{ onReset: () => void; children: ReactNode }, { error: Error | null }> {
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

export function App() {
  const [route, setRoute] = useState<Route>({ page: "home" });

  useEffect(() => {
    const onReject = (e: PromiseRejectionEvent) => toast(errorMessage(e.reason), "error");
    window.addEventListener("unhandledrejection", onReject);
    return () => window.removeEventListener("unhandledrejection", onReject);
  }, []);

  return (
    <ErrorBoundary onReset={() => setRoute({ page: "home" })}>
      {route.page === "home" ? (
        <Home
          startWithNew={route.newProject ?? false}
          onOpen={(id, firstMessage) => setRoute({ page: "editor", id, firstMessage })}
        />
      ) : (
        <Editor
          key={route.id}
          projectId={route.id}
          firstMessage={route.firstMessage}
          onHome={(newProject) => setRoute({ page: "home", newProject })}
          onOpen={(id) => setRoute({ page: "editor", id })}
        />
      )}
      <ToastHost />
    </ErrorBoundary>
  );
}

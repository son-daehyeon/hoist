import type { AgentEvent } from "./bindings";

type AgentEventHandler = (event: AgentEvent) => void;

const handlers = new Set<AgentEventHandler>();

export const emitAgentEvent = (event: AgentEvent) => handlers.forEach((h) => h(event));

export function onAgentEvent(handler: AgentEventHandler) {
  handlers.add(handler);
  return () => {
    handlers.delete(handler);
  };
}

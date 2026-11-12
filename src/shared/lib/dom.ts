export const isTextEntry = (t: EventTarget | null) =>
  t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement;

export const isFormField = (t: EventTarget | null) => isTextEntry(t) || t instanceof HTMLSelectElement;

export const dialogOpen = () => document.querySelector("[role=dialog]") !== null;

export const modalOpen = () => document.querySelector(".bs-dialog-backdrop:not(.is-leaving)") !== null;

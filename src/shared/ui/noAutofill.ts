// Edge/WebView2 address autofill can ignore autocomplete="off" alone; a neutral name and data-form-type keep its heuristics from matching.
export const noAutofill = { autoComplete: "off", name: "hoist-field", "data-form-type": "other" } as const;

export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "scope-enum": [
      2,
      "always",
      ["core", "tauri", "agent", "ui", "installer", "design", "docs", "deps", "ci", "release"],
    ],
    "header-max-length": [2, "always", 100],
    "subject-case": [0],
  },
};

import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import boundaries from "eslint-plugin-boundaries";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

const LAYERS = ["app", "pages", "widgets", "features", "entities", "shared"];
const ENTRY = ["index.ts", "index.tsx", "*.css"];

const lower = (layer) => LAYERS.slice(LAYERS.indexOf(layer) + 1);

export default tseslint.config(
  {
    ignores: [
      "**/dist/**",
      "**/target/**",
      "**/src-tauri/gen/**",
      "**/node_modules/**",
      "src/shared/api/bindings.ts",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  reactHooks.configs.flat.recommended,
  reactRefresh.configs.vite,
  {
    files: ["src/**/*.{ts,tsx}"],
    plugins: { boundaries },
    settings: {
      "import/resolver": { typescript: { project: "./tsconfig.json" } },
      "boundaries/include": ["src/**/*"],
      "boundaries/elements": [
        { type: "app", pattern: "src/app" },
        { type: "pages", pattern: "src/pages/*", capture: ["slice"] },
        { type: "widgets", pattern: "src/widgets/*", capture: ["slice"] },
        { type: "features", pattern: "src/features/*", capture: ["slice"] },
        { type: "entities", pattern: "src/entities/*", capture: ["slice"] },
        { type: "shared", pattern: "src/shared/*", capture: ["segment"] },
      ],
    },
    rules: {
      "boundaries/no-unknown-files": "error",
      "boundaries/no-unknown-dependencies": "error",
      "boundaries/dependencies": [
        "error",
        {
          default: "disallow",
          message:
            "FSD 위반: {{from.element.type}}/{{from.element.captured.slice}}{{from.element.captured.segment}} → {{to.element.type}}/{{to.element.captured.slice}}{{to.element.captured.segment}} ({{to.internalPath}})",
          policies: [
            { allow: { dependency: { relationship: { to: "internal" } } } },
            ...LAYERS.filter((l) => l !== "shared").map((layer) => ({
              from: { element: { type: layer } },
              allow: { to: { element: { types: lower(layer), fileInternalPath: ENTRY } } },
            })),
            {
              from: { element: { type: "shared" } },
              allow: { to: { element: { type: "shared", fileInternalPath: ENTRY } } },
            },
          ],
        },
      ],
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@tauri-apps/*"],
              message:
                "Tauri API는 src/shared/api에서만 import하고, 나머지는 shared/api가 내보내는 함수를 쓴다.",
            },
          ],
        },
      ],
    },
  },
  {
    files: ["src/shared/api/**/*.{ts,tsx}"],
    rules: { "no-restricted-imports": "off" },
  },
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_", varsIgnorePattern: "^_" }],
      "@typescript-eslint/consistent-type-imports": "error",
    },
  },
  prettier,
);

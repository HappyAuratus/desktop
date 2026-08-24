import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";
import { defineConfig, globalIgnores } from "eslint/config";

export default defineConfig([
  globalIgnores(["dist"]),
  {
    files: ["**/*.{ts,tsx}"],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      globals: globals.browser,
    },
  },
  {
    // Library barrels mix components with hooks and types; Vite fast-refresh
    // export rules apply to app entrypoints, not shared packages.
    files: ["packages/editor/**/*.{ts,tsx}"],
    rules: {
      "react-refresh/only-export-components": "off",
    },
  },
  {
    files: ["packages/app-shell/src/**/*.test.{ts,tsx}"],
    rules: {
      "no-restricted-properties": [
        "error",
        {
          object: "userEvent",
          property: "setup",
          message:
            "Use setupUser() (real input gaps) or setupTypingUser() (typing-heavy ProseMirror tests) from src/test/user.ts; raw userEvent.setup() makes scheduling choices implicit and has caused timer-starvation flakes and zombie keystroke chains.",
        },
      ],
    },
  },
  {
    files: ["packages/app-shell/src/**/*.tsx"],
    rules: {
      "no-restricted-syntax": [
        "error",
        {
          selector:
            "JSXExpressionContainer MemberExpression:not([computed=true])[property.name='message']",
          message:
            "Do not render Error.message in the UI; use localizeContractError or an approved localized validation message.",
        },
      ],
    },
  },
]);

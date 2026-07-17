import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import prettier from "eslint-plugin-prettier/recommended";
import tseslint from "typescript-eslint";
import security from "eslint-plugin-security";

export default tseslint.config(
  // Build output and Istanbul's generated lcov-report bundle are not our code.
  // `scripts` are Node build tooling (release manifests, blog prerender) linted
  // with Node globals rather than the browser preset used for `src`/`tests`.
  { ignores: ["dist", "coverage", "src/generated"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended, prettier],
    files: ["src/**/*.{ts,tsx}", "tests/**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
      security,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
      // eslint-plugin-security — the same ruleset bulwark enforces in CI. Kept
      // here too so a finding surfaces in `yarn lint` while the code is being
      // written, and so `eslint-disable-next-line security/...` comments that
      // record reviewed false positives resolve to a rule that actually exists.
      ...security.configs.recommended.rules,
    },
  },
  {
    // Build-time Node scripts: Node globals, no React/browser rules.
    extends: [js.configs.recommended, ...tseslint.configs.recommended, prettier],
    files: ["scripts/**/*.ts", "vite.config.ts"],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.node,
    },
    plugins: { security },
    rules: {
      ...security.configs.recommended.rules,
    },
  },
);

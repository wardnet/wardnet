import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import prettier from "eslint-plugin-prettier/recommended";
import tseslint from "typescript-eslint";
import security from "eslint-plugin-security";

export default tseslint.config(
  // `coverage` holds Istanbul's generated lcov-report bundle, which is not our
  // code and carries its own eslint-disable headers.
  { ignores: ["dist", "coverage"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended, prettier],
    files: ["**/*.{ts,tsx}"],
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
      // eslint-plugin-security, the same ruleset bulwark enforces in CI. Kept
      // here too so a finding surfaces in `yarn lint` while the code is being
      // written rather than for the first time in CI — and so the
      // `eslint-disable-next-line security/...` comments that record reviewed
      // false positives resolve to a rule that actually exists.
      ...security.configs.recommended.rules,
    },
  },
);

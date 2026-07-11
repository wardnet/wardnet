import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
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
      security,
    },
    rules: {
      // eslint-plugin-security, the same ruleset bulwark enforces in CI. Kept
      // here too so a finding surfaces in `yarn lint` while you're writing the
      // code, not for the first time in CI — and so the
      // `eslint-disable-next-line security/...` comments that record reviewed
      // false positives resolve to a rule that actually exists.
      ...security.configs.recommended.rules,
      // The v7 `refs` rule flags two intentional idioms this library uses —
      // the "latest ref" (`useUpdateDevice`) and the "latch ref"
      // (`ConnectionGate`). Those are silenced with scoped
      // `eslint-disable-next-line react-hooks/refs` at their call sites so the
      // rule stays active for the rest of the package.
      ...reactHooks.configs.recommended.rules,
    },
  },
);

import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import prettier from "eslint-plugin-prettier/recommended";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended, prettier],
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      // The v7 `refs` rule flags two intentional idioms this library uses —
      // the "latest ref" (`useUpdateDevice`) and the "latch ref"
      // (`ConnectionGate`). Those are silenced with scoped
      // `eslint-disable-next-line react-hooks/refs` at their call sites so the
      // rule stays active for the rest of the package.
      ...reactHooks.configs.recommended.rules,
    },
  },
);

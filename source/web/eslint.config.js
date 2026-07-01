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
      ...reactHooks.configs.recommended.rules,
      // `@wardnet/web` is a hooks/component library that leans on two
      // intentional ref idioms the v7 `refs` rule flags: the "latest ref"
      // (assign `ref.current = latest` in render so a later callback reads the
      // freshest value, e.g. `useUpdateDevice`) and the "latch ref" (read a
      // ref in render to permanently unblock, e.g. `ConnectionGate`). Both are
      // documented React patterns, so keep `rules-of-hooks`/`exhaustive-deps`
      // on but silence `refs`.
      "react-hooks/refs": "off",
    },
  },
);

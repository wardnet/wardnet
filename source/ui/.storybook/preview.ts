import type { Preview } from "@storybook/react-vite";
import "./preview.css";

/**
 * Bridge Storybook 10's built-in color-scheme toolbar toggle to the Wardnet
 * design tokens, which key off `[data-theme="dark"]`. We deliberately do NOT
 * register our own `theme` global — that produced a second, duplicate
 * light/dark switcher next to Storybook's built-in one.
 */
function resolveDark(scheme: unknown): boolean {
  if (scheme === "dark") return true;
  if (scheme === "light") return false;
  // "system" / undefined → follow the OS preference.
  return (
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: dark)").matches === true
  );
}

const preview: Preview = {
  parameters: {
    controls: {
      matchers: { color: /(background|color)$/i, date: /Date$/i },
    },
  },
  decorators: [
    (Story, context) => {
      const g = context.globals as Record<string, unknown>;
      const scheme = g.colorScheme ?? g.theme;
      document.documentElement.setAttribute(
        "data-theme",
        resolveDark(scheme) ? "dark" : "light",
      );
      return Story();
    },
  ],
};

export default preview;

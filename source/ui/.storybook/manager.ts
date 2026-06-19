import { addons } from "storybook/manager-api";
import { create } from "storybook/theming";

// Replace Storybook's own branding in the sidebar header with the Wardnet
// lockup. The logo is served from src/assets via `staticDirs` in main.ts, so
// it resolves at the manager root. The light-surface variant (ink + emerald)
// is used because the manager runs on Storybook's default light chrome.
const theme = create({
  base: "light",
  brandTitle: "Wardnet Design System",
  brandImage: "./wardnet-logo-light.svg",
  brandTarget: "_self",
  // Brand accent matches the design tokens' emerald (--accent).
  colorPrimary: "#12b981",
  colorSecondary: "#12b981",
});

addons.setConfig({ theme });

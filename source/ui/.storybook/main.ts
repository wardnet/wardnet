import type { StorybookConfig } from "@storybook/react-vite";
import tailwindcss from "@tailwindcss/vite";

const config: StorybookConfig = {
  stories: ["../src/**/*.stories.@(ts|tsx)", "../src/**/*.mdx"],
  addons: ["@storybook/addon-docs", "@storybook/addon-mcp"],
  // Serve the brand assets at the manager root so the custom theme's
  // brandImage (.storybook/manager.ts) can reference the Wardnet logo.
  staticDirs: ["../src/assets"],
  // Hide the built-in "Get started" onboarding checklist from the sidebar.
  features: {
    sidebarOnboardingChecklist: false,
  },
  framework: {
    name: "@storybook/react-vite",
    options: {},
  },
  viteFinal: async (viteConfig) => {
    viteConfig.plugins = viteConfig.plugins ?? [];
    viteConfig.plugins.push(tailwindcss());
    return viteConfig;
  },
};

export default config;

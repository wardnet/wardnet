/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import yaml from "@modyfi/vite-plugin-yaml";
// biome-ignore lint/correctness/noNodejsModules: build config, executed by Node at build time and never bundled
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  // biome-ignore lint/correctness/noProcessGlobal: build config, executed by Node at build time and never bundled
  base: process.env.VITE_BASE_PATH || "/",
  plugins: [react(), tailwindcss(), yaml()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 7415,
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: "./tests/setup.ts",
    css: false,
    reporters: ["default", "junit"],
    outputFile: {
      junit: "./test-results/junit.xml",
    },
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "cobertura", "json-summary"],
      reportsDirectory: "./coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/main.tsx"],
    },
  },
});

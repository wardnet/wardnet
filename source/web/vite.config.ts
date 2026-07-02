/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @wardnet/web ships as source (no bundle of its own — consumers build it),
// so this config exists purely to run its Vitest suite: the React plugin
// transforms JSX in the component tests and the `test` block wires up jsdom
// + coverage the same way the app packages do.
export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    // CI runners are slower and coverage instrumentation adds load; give
    // userEvent-driven interaction tests headroom over the 5s default.
    testTimeout: 20000,
    environment: "jsdom",
    setupFiles: "./tests/setup.ts",
    css: false,
    reporters: ["default", "junit"],
    outputFile: {
      junit: "./test-results/junit.xml",
    },
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "cobertura"],
      reportsDirectory: "./coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/index.ts",
        "src/vite-env.d.ts",
        "src/**/*.d.ts",
        "src/lib/registerSW.ts",
      ],
    },
  },
});

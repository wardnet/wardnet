/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";

// The SDK is a plain TypeScript library with no bundler of its own (tsup
// produces the published dist/). This config exists purely to run the Vitest
// suite in a Node environment — the services under test talk to a stubbed
// global WebSocket, so no DOM is required.
export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    include: ["tests/**/*.test.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov", "json-summary"],
      reportsDirectory: "./coverage",
      include: ["src/**/*.ts"],
      exclude: ["src/index.ts", "src/consola.ts", "src/**/*.d.ts", "src/types/**"],
    },
  },
});

/// <reference types="vitest/config" />
import path from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "/admin/",
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
    preserveSymlinks: true,
  },
  server: {
    port: 7412,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:7411",
        ws: true,
      },
    },
  },
  optimizeDeps: {
    // Pre-bundle the CommonJS deps that the excluded workspace packages import
    // (cronstrue via @wardnet/web, consola via @wardnet/js): excluding a
    // linked package stops Vite bundling its deps, so its CJS deps must be
    // pre-bundled explicitly or their `default` export won't be interop'd.
    include: ["use-sync-external-store/shim", "cronstrue", "consola"],
    // Our own source workspace packages (linked via Yarn `portal:`) must NOT be
    // pre-bundled: Vite caches a pre-bundle in node_modules/.vite/deps and does
    // not invalidate it when a portal dep's *source* changes, so edits to these
    // packages surface as stale "does not provide an export named X" errors until
    // the cache is force-cleared. Excluding them makes Vite read their source
    // directly every time (and keeps HMR working across the workspace).
    exclude: ["@wardnet/web", "@wardnet/js"],
  },
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
      reporter: ["text", "lcov", "cobertura", "json-summary"],
      reportsDirectory: "./coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/main.tsx",
        "src/vite-env.d.ts",
        "src/**/*.d.ts",
        "src/sw.ts",
      ],
    },
  },
});

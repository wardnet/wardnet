/// <reference types="vitest/config" />
// biome-ignore lint/correctness/noNodejsModules: build config, executed by Node at build time and never bundled
import path from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "/admin/",
  resolve: {
    alias: {
      // biome-ignore lint/correctness/noGlobalDirnameFilename: build config, executed by Node at build time and never bundled
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
    watch: {
      // Our source workspace packages are Yarn-linked into node_modules, which
      // Vite's file watcher ignores by default — so edits to @wardnet/web
      // (shared components/hooks) and @wardnet/js never triggered HMR, only a
      // server restart picked them up. Un-ignore them so cross-package HMR
      // works. (They're already excluded from optimizeDeps so Vite reads their
      // source directly.)
      ignored: [
        "!**/node_modules/@wardnet/web/**",
        "!**/node_modules/@wardnet/js/**",
      ],
    },
  },
  optimizeDeps: {
    // Pre-bundle the CommonJS deps that the excluded workspace packages import
    // (cronstrue + qrcode via @wardnet/web, consola via @wardnet/js): excluding
    // a linked package stops Vite bundling its deps, so its CJS deps must be
    // pre-bundled explicitly or their `default`/named exports won't be
    // interop'd — qrcode's "browser" field swaps in a plain-CJS file with
    // internal `require()` calls that throw `ReferenceError: require is not
    // defined` when Vite serves it unbundled.
    include: ["use-sync-external-store/shim", "cronstrue", "consola", "qrcode"],
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

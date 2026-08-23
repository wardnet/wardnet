/// <reference types="vitest/config" />
// biome-ignore lint/correctness/noNodejsModules: build config, executed by Node at build time and never bundled
import path from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // The PWA/service-worker plugin has no role in unit tests and its
    // injectManifest build hook only gets in the way there — skip it
    // when Vitest is driving the config.
    // biome-ignore lint/correctness/noProcessGlobal: build config, executed by Node at build time and never bundled
    ...(process.env.VITEST
      ? []
      : [
          VitePWA({
            strategies: "injectManifest",
            srcDir: "src",
            filename: "sw.ts",
            // Registration is handled manually via @wardnet/web's registerSW
            injectRegister: false,
            // Use our own public/manifest.json
            manifest: false,
            devOptions: {
              enabled: false,
            },
          }),
        ]),
  ],
  base: "/app/",
  resolve: {
    alias: {
      // biome-ignore lint/correctness/noGlobalDirnameFilename: build config, executed by Node at build time and never bundled
      "@": path.resolve(__dirname, "src"),
    },
    preserveSymlinks: true,
  },
  server: {
    port: 7413,
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
    // CJS deps of the excluded workspace packages (see admin-site/web config).
    include: ["cronstrue", "consola", "qrcode"],
    // See admin-site/web/vite.config.ts: don't pre-bundle our own source
    // workspace packages, so source edits aren't masked by a stale .vite cache.
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

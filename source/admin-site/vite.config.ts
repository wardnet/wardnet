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
    // (cronstrue via @wardnet/wardnet-web, consola via @wardnet/js): excluding a
    // linked package stops Vite bundling its deps, so its CJS deps must be
    // pre-bundled explicitly or their `default` export won't be interop'd.
    include: ["use-sync-external-store/shim", "cronstrue", "consola"],
    // Our own source workspace packages (linked via Yarn `portal:`) must NOT be
    // pre-bundled: Vite caches a pre-bundle in node_modules/.vite/deps and does
    // not invalidate it when a portal dep's *source* changes, so edits to these
    // packages surface as stale "does not provide an export named X" errors until
    // the cache is force-cleared. Excluding them makes Vite read their source
    // directly every time (and keeps HMR working across the workspace).
    exclude: ["@wardnet/wardnet-web", "@wardnet/js"],
  },
});

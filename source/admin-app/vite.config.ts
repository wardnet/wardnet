import path from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    VitePWA({
      strategies: "injectManifest",
      srcDir: "src",
      filename: "sw.ts",
      // Registration is handled manually via @wardnet/wardnet-web's registerSW
      injectRegister: false,
      // Use our own public/manifest.json
      manifest: false,
      devOptions: {
        enabled: false,
      },
    }),
  ],
  base: "/admin-app/",
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
    preserveSymlinks: true,
  },
  server: {
    port: 7414,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:7411",
        ws: true,
      },
      "/admin/": {
        target: "http://127.0.0.1:7412",
      },
    },
  },
  optimizeDeps: {
    // CJS deps of the excluded workspace packages (see admin-site/web config).
    include: ["use-sync-external-store/shim", "cronstrue", "consola"],
    // See admin-site/web/vite.config.ts: don't pre-bundle our own source
    // workspace packages, so source edits aren't masked by a stale .vite cache.
    exclude: ["@wardnet/wardnet-web", "@wardnet/js"],
  },
});

import path from "path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
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
  },
});

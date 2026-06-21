import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import dts from "vite-plugin-dts";

// Everything that is a dependency or peer dependency is kept external — the
// design system ships only its own source; consumers provide React, Radix,
// lucide, etc. (and the design tokens via @wardnet/styles).
const external = [
  /^react($|\/)/,
  /^react-dom($|\/)/,
  /^radix-ui($|\/)/,
  /^lucide-react($|\/)/,
  "clsx",
  "class-variance-authority",
  "cmdk",
  /^@wardnet\/styles($|\/)/,
];

export default defineConfig({
  plugins: [
    react(),
    dts({
      include: ["src"],
      exclude: ["src/**/*.stories.tsx"],
      // Strip the src/ prefix so the entry lands at dist/index.d.ts to match
      // the package "types" field.
      entryRoot: "src",
    }),
  ],
  build: {
    lib: {
      entry: resolve(import.meta.dirname, "src/index.ts"),
      formats: ["es"],
      fileName: () => "index.js",
      cssFileName: "ui",
    },
    cssCodeSplit: false,
    sourcemap: true,
    rollupOptions: {
      external,
    },
  },
});

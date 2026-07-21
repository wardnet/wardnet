import { defineConfig } from "tsup";

export default defineConfig({
  // `consola` is a separate entry so the main bundle stays dependency-free;
  // it's an optional peer, pulled in only by consumers importing this subpath.
  entry: ["src/index.ts", "src/consola.ts"],
  format: ["esm"],
  target: "es2020",
  dts: true,
  sourcemap: true,
  clean: true,
  external: ["consola"],
});

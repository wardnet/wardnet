import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/**/*.spec.ts"],
    // Compose health waits + first-boot setup-wizard pushes a single spec
    // past the 5 s default. Generous ceilings keep flake from a slow
    // GitHub runner from masking real failures.
    testTimeout: 60_000,
    hookTimeout: 120_000,
    reporters: [
      "default",
      ["junit", { outputFile: "./reports/junit.xml" }],
      ["json", { outputFile: "./reports/results.json" }],
    ],
  },
});

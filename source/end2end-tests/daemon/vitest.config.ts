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
    // All specs run against one shared daemon container, so file-level
    // parallelism is hostile (race on setup wizard, race on DHCP
    // toggle, race on lease state). singleFork keeps every file in one
    // process — module-level state in helpers.ts (ADMIN_PASSWORD) is
    // shared across files, and tests execute serially.
    pool: "forks",
    poolOptions: {
      forks: {
        singleFork: true,
      },
    },
    reporters: [
      "default",
      ["junit", { outputFile: "./reports/junit.xml" }],
      ["json", { outputFile: "./reports/results.json" }],
    ],
  },
});

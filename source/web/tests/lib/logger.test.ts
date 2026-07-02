import { describe, expect, it } from "vitest";
import { logger, createLogger } from "../../src/lib/logger";

describe("logger", () => {
  it("exports a ready-to-use logger instance", () => {
    expect(logger).toBeDefined();
    expect(typeof logger.info).toBe("function");
  });
  it("re-exports the createLogger factory", () => {
    const custom = createLogger("test.scope");
    expect(typeof custom.info).toBe("function");
  });
});

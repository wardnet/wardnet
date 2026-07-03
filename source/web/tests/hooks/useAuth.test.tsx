import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useAuth } from "../../src/hooks/useAuth";

describe("useAuth", () => {
  it("exposes the auth store state and actions", () => {
    const { result } = renderHook(() => useAuth());
    expect(typeof result.current.login).toBe("function");
    expect(typeof result.current.logout).toBe("function");
    expect(typeof result.current.checkAuth).toBe("function");
    expect("isAdmin" in result.current).toBe(true);
  });
});

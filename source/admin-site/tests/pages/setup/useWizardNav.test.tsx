import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAdvanceWizard } = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAdvanceWizard };
});

import { useWizardNav } from "@/pages/setup/useWizardNav";

const advanceMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({
    mutate: advanceMutate,
    isPending: false,
    isError: false,
  });
});

describe("useWizardNav", () => {
  it("goNext advances to the next step in the canonical order", () => {
    const { result } = renderHook(() => useWizardNav("network"));
    result.current.goNext();
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "dhcp" });
  });

  it("goNext forwards wizard_mode when provided", () => {
    const { result } = renderHook(() => useWizardNav("dhcp"));
    result.current.goNext({ wizard_mode: "primary" });
    expect(advanceMutate).toHaveBeenCalledWith({
      to_step: "router_mac",
      wizard_mode: "primary",
    });
  });

  it("goNext is a no-op on the terminal step", () => {
    const { result } = renderHook(() => useWizardNav("completed"));
    result.current.goNext();
    expect(advanceMutate).not.toHaveBeenCalled();
  });

  it("goBack rewinds to the previous step", () => {
    const { result } = renderHook(() => useWizardNav("dns"));
    result.current.goBack();
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "router_mac" });
  });

  it("goBack refuses to cross the rewind floor", () => {
    const { result } = renderHook(() => useWizardNav("network"));
    result.current.goBack();
    expect(advanceMutate).not.toHaveBeenCalled();
  });

  it("goTo refuses forward jumps", () => {
    const { result } = renderHook(() => useWizardNav("dhcp"));
    result.current.goTo("review");
    expect(advanceMutate).not.toHaveBeenCalled();
  });

  it("goTo refuses rewinding to admin", () => {
    const { result } = renderHook(() => useWizardNav("review"));
    result.current.goTo("admin");
    expect(advanceMutate).not.toHaveBeenCalled();
  });

  it("goTo rewinds to an earlier step at or above the floor", () => {
    const { result } = renderHook(() => useWizardNav("review"));
    result.current.goTo("dns");
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "dns" });
  });
});

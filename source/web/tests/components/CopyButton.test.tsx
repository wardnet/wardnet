import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { toast } = vi.hoisted(() => ({
  toast: { error: vi.fn() },
}));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import { CopyButton } from "../../src/components/CopyButton";

describe("CopyButton", () => {
  const writeText = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    writeText.mockClear();
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
  });

  afterEach(() => vi.restoreAllMocks());

  it("copies the value and flips to a confirmation", async () => {
    render(<CopyButton value="tok.abc.my.wardnet.services" />);

    fireEvent.click(screen.getByRole("button", { name: "Copy" }));

    expect(writeText).toHaveBeenCalledWith("tok.abc.my.wardnet.services");
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  it("honours a custom label", () => {
    render(<CopyButton value="x" label="Copy hostname" />);
    expect(
      screen.getByRole("button", { name: "Copy hostname" }),
    ).toBeInTheDocument();
  });

  it("toasts an error when the clipboard write fails", async () => {
    writeText.mockRejectedValueOnce(new Error("denied"));
    render(<CopyButton value="x" />);

    fireEvent.click(screen.getByRole("button", { name: "Copy" }));

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        "Couldn't copy to the clipboard",
      ),
    );
    // It never flips to the confirmation on failure.
    expect(screen.queryByText("Copied")).not.toBeInTheDocument();
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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
});

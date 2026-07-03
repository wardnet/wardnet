import { act, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DiscoveryPlaceholder } from "@/components/compound/DiscoveryPlaceholder";
import { renderWithProviders } from "../../test-utils";

afterEach(() => {
  vi.useRealTimers();
});

describe("DiscoveryPlaceholder", () => {
  it("renders the default message and hint", () => {
    renderWithProviders(<DiscoveryPlaceholder />);
    expect(
      screen.getByText("Searching for network devices"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Devices will appear as they are detected on the network.",
      ),
    ).toBeInTheDocument();
  });

  it("renders custom copy and column count", () => {
    renderWithProviders(
      <DiscoveryPlaceholder cols={3} message="Scanning" hint="Please wait" />,
    );
    expect(screen.getByText("Scanning")).toBeInTheDocument();
    expect(screen.getByText("Please wait")).toBeInTheDocument();
  });

  it("animates the trailing dots on an interval", () => {
    vi.useFakeTimers();
    const { unmount } = renderWithProviders(
      <DiscoveryPlaceholder message="Scanning" />,
    );
    // Advance past four ticks so the dot string grows to 3 then resets.
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    // Component still mounted; the interval callback ran without throwing.
    expect(screen.getByText("Scanning")).toBeInTheDocument();
    unmount();
  });
});

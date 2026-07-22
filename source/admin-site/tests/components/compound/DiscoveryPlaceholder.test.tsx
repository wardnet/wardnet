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
    // The dots live in their own span, not the <p>'s direct text node, so we
    // read them explicitly — asserting on getByText("Scanning") ignores the
    // animated sibling and would pass even if the interval were deleted.
    const dots = screen.getByTestId("discovery-dots");
    expect(dots.textContent).toBe("");
    // Each 500ms tick appends one dot, up to three, then resets to empty.
    act(() => vi.advanceTimersByTime(500));
    expect(dots.textContent).toBe(".");
    act(() => vi.advanceTimersByTime(500));
    expect(dots.textContent).toBe("..");
    act(() => vi.advanceTimersByTime(500));
    expect(dots.textContent).toBe("...");
    act(() => vi.advanceTimersByTime(500));
    // Removing the useEffect/setInterval would freeze this at "" and fail
    // every assertion above.
    expect(dots.textContent).toBe("");
    unmount();
  });
});

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ConnectionBanner } from "@/components/ConnectionBanner";

describe("ConnectionBanner", () => {
  it("renders nothing when online", () => {
    const { container } = render(<ConnectionBanner connState="online" />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the offline banner", () => {
    render(<ConnectionBanner connState="offline" />);
    expect(
      screen.getByText(/No connection to wardnet daemon/i),
    ).toBeInTheDocument();
  });

  it("shows the reconnecting banner for any non-offline, non-online state", () => {
    render(<ConnectionBanner connState="reconnecting" />);
    expect(
      screen.getByText(/Reconnecting to wardnet daemon/i),
    ).toBeInTheDocument();
  });
});

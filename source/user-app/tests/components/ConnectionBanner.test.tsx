import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";

import { ConnectionBanner } from "../../src/components/ConnectionBanner";

describe("ConnectionBanner", () => {
  it("renders nothing when online", () => {
    const { container } = render(<ConnectionBanner connState="online" />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the offline banner as an alert", () => {
    render(<ConnectionBanner connState="offline" />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      /No connection to wardnet daemon/i,
    );
  });

  it("shows the reconnecting banner", () => {
    render(<ConnectionBanner connState="reconnecting" />);
    expect(screen.getByText(/Reconnecting to wardnet daemon/i)).toBeInTheDocument();
  });
});

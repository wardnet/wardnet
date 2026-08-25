import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AccessRequestStatusPill } from "../../src/components/AccessRequestStatusPill";

describe("AccessRequestStatusPill", () => {
  it("labels each status", () => {
    const { rerender } = render(<AccessRequestStatusPill status="approved" />);
    expect(screen.getByText("Approved")).toBeInTheDocument();
    // "Declined" rather than "Rejected": the household member reads this on
    // their own device, and the softer word is the one the PWA copy uses.
    rerender(<AccessRequestStatusPill status="rejected" />);
    expect(screen.getByText("Declined")).toBeInTheDocument();
    rerender(<AccessRequestStatusPill status="pending" />);
    expect(screen.getByText("Pending")).toBeInTheDocument();
  });
});

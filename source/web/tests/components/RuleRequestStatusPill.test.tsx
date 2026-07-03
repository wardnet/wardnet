import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RuleRequestStatusPill } from "../../src/components/RuleRequestStatusPill";

describe("RuleRequestStatusPill", () => {
  it("labels each status", () => {
    const { rerender } = render(<RuleRequestStatusPill status="approved" />);
    expect(screen.getByText("Approved")).toBeInTheDocument();
    rerender(<RuleRequestStatusPill status="rejected" />);
    expect(screen.getByText("Rejected")).toBeInTheDocument();
    rerender(<RuleRequestStatusPill status="pending" />);
    expect(screen.getByText("Pending")).toBeInTheDocument();
  });
});

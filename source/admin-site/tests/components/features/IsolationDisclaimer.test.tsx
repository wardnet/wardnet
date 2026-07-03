import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  IsolationDisclaimer,
  ISOLATION_DISCLAIMER,
} from "@/components/features/IsolationDisclaimer";

describe("IsolationDisclaimer", () => {
  it("renders the honest isolation disclaimer text", () => {
    render(<IsolationDisclaimer />);
    expect(screen.getByText(ISOLATION_DISCLAIMER)).toBeInTheDocument();
    expect(ISOLATION_DISCLAIMER).toMatch(/client isolation/i);
    expect(ISOLATION_DISCLAIMER).toMatch(/member isolation/i);
  });
});

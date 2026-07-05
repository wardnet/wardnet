import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Premium } from "@/components/layouts/Premium";

const ACCOUNT_URL = "https://account.wardnet.network";

describe("Premium section", () => {
  it("renders both the Free and Premium columns", () => {
    render(<Premium />);
    expect(screen.getByRole("heading", { name: "Free" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Premium" })).toBeInTheDocument();
  });

  it("uses the standardized product names", () => {
    render(<Premium />);
    expect(screen.getByText(/Unlock dynamic DNS and secure remote tunneling/i)).toBeInTheDocument();
    expect(
      screen.getByText(/Premium gateway features for self-hosted Wardnet/i),
    ).toBeInTheDocument();
  });

  it("shows the free-trial trust line", () => {
    render(<Premium />);
    expect(screen.getByText(/free trial · cancel anytime/i)).toBeInTheDocument();
  });

  it("points the CTA at the cloud account SPA", () => {
    render(<Premium />);
    expect(screen.getByRole("link", { name: "Start free trial" })).toHaveAttribute(
      "href",
      ACCOUNT_URL,
    );
    expect(screen.getByRole("link", { name: "See plans" })).toHaveAttribute("href", ACCOUNT_URL);
  });

  it("does not publish any prices or demo plan names", () => {
    render(<Premium />);
    expect(screen.queryByText(/\$\d/)).not.toBeInTheDocument();
    expect(screen.queryByText(/\bBasic\b|\bPro\b|\bTeam\b/)).not.toBeInTheDocument();
  });
});

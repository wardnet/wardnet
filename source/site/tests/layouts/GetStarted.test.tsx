import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GetStarted } from "@/components/layouts/GetStarted";

describe("GetStarted", () => {
  it("renders the section title", () => {
    render(<GetStarted />);
    expect(screen.getByText("Get started")).toBeInTheDocument();
  });

  it("renders the install command", () => {
    render(<GetStarted />);
    expect(screen.getByText("curl -sSL https://wardnet.network/install.sh | bash")).toBeInTheDocument();
  });

  it("renders the GitHub link", () => {
    render(<GetStarted />);
    const link = screen.getByRole("link", { name: "View on GitHub" });
    expect(link).toHaveAttribute("href", "https://github.com/wardnet/wardnet");
  });
});

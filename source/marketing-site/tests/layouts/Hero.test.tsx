import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Hero } from "@/components/layouts/Hero";

const noop = () => {};

describe("Hero", () => {
  it("renders the heading", () => {
    render(<Hero onExplore={noop} />);
    expect(screen.getByRole("heading", { name: "Wardnet" })).toBeInTheDocument();
  });

  it("renders the tagline", () => {
    render(<Hero onExplore={noop} />);
    expect(screen.getByText("Your network. Your rules.")).toBeInTheDocument();
  });

  it("keeps the cloud-optional / LAN-first promise", () => {
    render(<Hero onExplore={noop} />);
    expect(screen.getByText(/no account required/i)).toBeInTheDocument();
    expect(screen.getByText(/nothing leaves your LAN/i)).toBeInTheDocument();
    expect(screen.getByText(/no cloud required/i)).toBeInTheDocument();
  });

  it("mentions Premium remote and mobile access", () => {
    render(<Hero onExplore={noop} />);
    expect(screen.getByText(/remote and mobile access.*via\s+Premium/i)).toBeInTheDocument();
  });

  it("renders the Download CTA with correct href", () => {
    render(<Hero onExplore={noop} />);
    const link = screen.getByRole("link", { name: "Download" });
    expect(link).toHaveAttribute("href", "https://github.com/wardnet/wardnet/releases");
  });

  it("renders the GitHub CTA with correct href", () => {
    render(<Hero onExplore={noop} />);
    const link = screen.getByRole("link", { name: "View on GitHub" });
    expect(link).toHaveAttribute("href", "https://github.com/wardnet/wardnet");
  });

  it("calls onExplore when the Explore button is clicked", async () => {
    const onExplore = vi.fn();
    render(<Hero onExplore={onExplore} />);
    await userEvent.click(screen.getByRole("button", { name: /scroll to features/i }));
    expect(onExplore).toHaveBeenCalledOnce();
  });
});

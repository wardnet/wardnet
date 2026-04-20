import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HowItWorks } from "@/components/layouts/HowItWorks";

describe("HowItWorks", () => {
  it("renders the section title", () => {
    render(<HowItWorks />);
    expect(screen.getByText("Up and running in minutes")).toBeInTheDocument();
  });

  it("renders all three steps", () => {
    render(<HowItWorks />);
    expect(screen.getByText("Install on your gateway")).toBeInTheDocument();
    expect(screen.getByText("Connect your devices")).toBeInTheDocument();
    expect(screen.getByText("Control from the dashboard")).toBeInTheDocument();
  });

  it("renders step numbers", () => {
    render(<HowItWorks />);
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });
});

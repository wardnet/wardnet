import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AppHeader } from "../../src/components/AppHeader";

describe("AppHeader", () => {
  it("shows the version and a connected indicator", () => {
    render(<AppHeader connState="online" version="2026.07.02" />);
    expect(screen.getByText("2026.07.02")).toBeInTheDocument();
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });
  it("shows reconnecting state", () => {
    render(<AppHeader connState="reconnecting" version={null} />);
    expect(screen.getByText("Reconnecting")).toBeInTheDocument();
  });
  it("shows offline state and omits version when null", () => {
    render(<AppHeader connState="offline" version={null} />);
    expect(screen.getByText("Offline")).toBeInTheDocument();
    expect(screen.queryByText("2026.07.02")).not.toBeInTheDocument();
  });
});

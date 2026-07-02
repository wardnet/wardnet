import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BusyOverlay } from "@/components/BusyOverlay";

describe("BusyOverlay", () => {
  it("shows the reboot working copy with a spinner", () => {
    const { container } = render(<BusyOverlay action="reboot" phase="working" />);
    expect(screen.getByText("Rebooting device…")).toBeInTheDocument();
    expect(screen.getByText("This takes about 60 seconds")).toBeInTheDocument();
    expect(container.querySelector(".animate-spin")).not.toBeNull();
  });

  it("shows the restart done copy with a check icon", () => {
    render(<BusyOverlay action="restart" phase="done" />);
    expect(screen.getByText("Daemon restarted")).toBeInTheDocument();
    expect(screen.getByText("Connection restored")).toBeInTheDocument();
    expect(screen.getByTestId("system-busy-overlay")).toBeInTheDocument();
  });

  it("shows restart working copy", () => {
    render(<BusyOverlay action="restart" phase="working" />);
    expect(screen.getByText("Restarting daemon…")).toBeInTheDocument();
  });

  it("shows reboot done copy", () => {
    render(<BusyOverlay action="reboot" phase="done" />);
    expect(screen.getByText("Device is back online")).toBeInTheDocument();
  });
});

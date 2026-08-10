import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { ConnectionStatus } from "@/components/compound/ConnectionStatus";
import { renderWithProviders } from "../../test-utils";

function dot(): HTMLElement {
  return document.querySelector("span.inline-block") as HTMLElement;
}

describe("ConnectionStatus", () => {
  it("shows Connecting while loading", () => {
    renderWithProviders(<ConnectionStatus isLoading reachable={false} />);
    expect(screen.getByText("Connecting…")).toBeInTheDocument();
    expect(dot().className).toContain("bg-warn");
  });

  it("shows Connected when reachable", () => {
    renderWithProviders(<ConnectionStatus isLoading={false} reachable />);
    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(dot().className).toContain("bg-accent");
  });

  it("shows Disconnected when not reachable", () => {
    renderWithProviders(
      <ConnectionStatus isLoading={false} reachable={false} />,
    );
    expect(screen.getByText("Disconnected")).toBeInTheDocument();
    expect(dot().className).toContain("bg-danger");
  });
});

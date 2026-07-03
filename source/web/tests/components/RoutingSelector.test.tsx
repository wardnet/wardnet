import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RoutingSelector } from "../../src/components/RoutingSelector";
import { makeTunnel, renderWithProviders } from "../test-utils";

describe("RoutingSelector", () => {
  beforeEach(() => vi.clearAllMocks());

  it("disables the control and links to /tunnels for admins when empty", () => {
    renderWithProviders(
      <RoutingSelector
        value={null}
        onChange={vi.fn()}
        tunnels={[]}
        isAdmin
        data-testid="routing"
      />,
    );
    expect(screen.getByTestId("routing")).toHaveAttribute("data-disabled", "");
    const link = screen.getByRole("link", { name: "Add one" });
    expect(link).toHaveAttribute("href", "/tunnels");
  });

  it("prompts non-admins to contact the admin when empty", () => {
    renderWithProviders(
      <RoutingSelector value={null} onChange={vi.fn()} tunnels={[]} />,
    );
    expect(screen.getByText(/Contact your network admin/)).toBeInTheDocument();
  });

  it("shows the selected tunnel label when the value resolves", () => {
    renderWithProviders(
      <RoutingSelector
        value={{ type: "tunnel", tunnel_id: "tun-1" }}
        onChange={vi.fn()}
        tunnels={[makeTunnel({ id: "tun-1", label: "Iceland" })]}
        data-testid="routing"
      />,
    );
    expect(screen.getByTestId("routing")).toHaveTextContent("Iceland");
  });

  it("falls back to Direct when the target tunnel is unknown", () => {
    renderWithProviders(
      <RoutingSelector
        value={{ type: "tunnel", tunnel_id: "missing" }}
        onChange={vi.fn()}
        tunnels={[makeTunnel({ id: "tun-1", label: "Iceland" })]}
        data-testid="routing"
      />,
    );
    expect(screen.getByTestId("routing")).toHaveTextContent("Direct");
  });

  it("emits a tunnel target when a tunnel is picked", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <RoutingSelector
        value={{ type: "direct" }}
        onChange={onChange}
        tunnels={[makeTunnel({ id: "tun-1", label: "Iceland" })]}
        data-testid="routing"
      />,
    );
    await user.click(screen.getByTestId("routing"));
    const option = await screen.findByRole("option", { name: /Iceland/ });
    await user.click(option);
    expect(onChange).toHaveBeenCalledWith({
      type: "tunnel",
      tunnel_id: "tun-1",
    });
  });

  it("emits a direct target when Direct is picked", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <RoutingSelector
        value={{ type: "tunnel", tunnel_id: "tun-1" }}
        onChange={onChange}
        tunnels={[makeTunnel({ id: "tun-1", label: "Iceland" })]}
        data-testid="routing"
      />,
    );
    await user.click(screen.getByTestId("routing"));
    const listbox = await screen.findByRole("listbox");
    await user.click(within(listbox).getByRole("option", { name: /Direct/ }));
    expect(onChange).toHaveBeenCalledWith({ type: "direct" });
  });
});

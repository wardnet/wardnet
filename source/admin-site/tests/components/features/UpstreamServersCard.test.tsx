import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { UpstreamDns } from "@wardnet/js";
import { UpstreamServersCard } from "@/components/features/UpstreamServersCard";
import { renderWithProviders } from "../../test-utils";

Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

const servers: UpstreamDns[] = [
  { name: "Cloudflare", address: "1.1.1.1", protocol: "udp" },
  { name: "Quad9", address: "9.9.9.9", protocol: "tls", port: 853 },
];

describe("UpstreamServersCard", () => {
  it("renders the empty state when there are no servers", () => {
    renderWithProviders(
      <UpstreamServersCard servers={[]} isSaving={false} onUpdate={vi.fn()} />,
    );
    expect(
      screen.getByText("No upstream servers configured."),
    ).toBeInTheDocument();
  });

  it("renders servers with address and protocol", () => {
    renderWithProviders(
      <UpstreamServersCard
        servers={servers}
        isSaving={false}
        onUpdate={vi.fn()}
      />,
    );
    expect(screen.getByText("Cloudflare")).toBeInTheDocument();
    expect(screen.getByText("9.9.9.9:853")).toBeInTheDocument();
    expect(screen.getByText("TLS")).toBeInTheDocument();
  });

  it("shows the fallback-only note in recursive mode", () => {
    renderWithProviders(
      <UpstreamServersCard
        servers={servers}
        isSaving={false}
        fallbackOnly
        onUpdate={vi.fn()}
      />,
    );
    expect(
      screen.getByText(/Recursive resolution is active/),
    ).toBeInTheDocument();
  });

  it("moves a server down via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onUpdate = vi.fn();
    renderWithProviders(
      <UpstreamServersCard
        servers={servers}
        isSaving={false}
        onUpdate={onUpdate}
      />,
    );
    const menus = screen.getAllByTestId("upstream-row-menu");
    await user.click(menus[0]);
    await user.click(
      await screen.findByRole("menuitem", { name: "Move down" }),
    );
    expect(onUpdate).toHaveBeenCalledWith([servers[1], servers[0]]);
  });

  it("ignores moving the first server up (out of bounds)", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onUpdate = vi.fn();
    renderWithProviders(
      <UpstreamServersCard
        servers={servers}
        isSaving={false}
        onUpdate={onUpdate}
      />,
    );
    const menus = screen.getAllByTestId("upstream-row-menu");
    await user.click(menus[0]);
    await user.click(await screen.findByRole("menuitem", { name: "Move up" }));
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("removes a server via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onUpdate = vi.fn();
    renderWithProviders(
      <UpstreamServersCard
        servers={servers}
        isSaving={false}
        onUpdate={onUpdate}
      />,
    );
    const menus = screen.getAllByTestId("upstream-row-menu");
    await user.click(menus[1]);
    await user.click(await screen.findByTestId("upstream-remove"));
    expect(onUpdate).toHaveBeenCalledWith([servers[0]]);
  });

  it("adds a plain UDP server through the inline form", async () => {
    const user = userEvent.setup();
    const onUpdate = vi.fn();
    renderWithProviders(
      <UpstreamServersCard servers={[]} isSaving={false} onUpdate={onUpdate} />,
    );
    await user.click(screen.getByTestId("upstream-add"));
    await user.type(screen.getByTestId("upstream-name"), "Google");
    await user.type(screen.getByTestId("upstream-address"), "8.8.8.8");
    await user.click(screen.getByTestId("upstream-submit"));
    expect(onUpdate).toHaveBeenCalledWith([
      {
        name: "Google",
        address: "8.8.8.8",
        protocol: "udp",
        port: undefined,
        tls_server_name: undefined,
      },
    ]);
  });

  it("requires a TLS server name when protocol is encrypted", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onUpdate = vi.fn();
    renderWithProviders(
      <UpstreamServersCard servers={[]} isSaving={false} onUpdate={onUpdate} />,
    );
    await user.click(screen.getByTestId("upstream-add"));
    // Pick the TLS protocol to reveal the SNI field.
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByRole("option", { name: "TLS" }));
    await user.type(screen.getByTestId("upstream-name"), "Quad9");
    await user.type(screen.getByTestId("upstream-address"), "9.9.9.9");
    // SNI field is now present.
    const sni = screen.getByLabelText("TLS server name");
    await user.type(sni, "dns.quad9.net");
    await user.click(screen.getByTestId("upstream-submit"));
    expect(onUpdate).toHaveBeenCalledWith([
      {
        name: "Quad9",
        address: "9.9.9.9",
        protocol: "tls",
        port: undefined,
        tls_server_name: "dns.quad9.net",
      },
    ]);
  });

  it("cancelling the add form hides it", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <UpstreamServersCard servers={[]} isSaving={false} onUpdate={vi.fn()} />,
    );
    await user.click(screen.getByTestId("upstream-add"));
    expect(screen.getByTestId("upstream-name")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("upstream-name")).not.toBeInTheDocument();
  });
});

import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";
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

type Props = ComponentProps<typeof UpstreamServersCard>;

/** Render with all required props defaulted (Failover mode so the up/down and
 *  remove row actions are present); override per test. Returns the resolved
 *  props so tests can assert against the mocks. */
function renderCard(overrides: Partial<Props> = {}): Props {
  const props: Props = {
    servers,
    isSaving: false,
    mode: "failover",
    onUpdate: vi.fn(),
    onModeChange: vi.fn(),
    onSelectServer: vi.fn(),
    ...overrides,
  };
  renderWithProviders(<UpstreamServersCard {...props} />);
  return props;
}

describe("UpstreamServersCard", () => {
  it("renders the empty state when there are no servers", () => {
    renderCard({ servers: [] });
    expect(
      screen.getByText("No upstream servers configured."),
    ).toBeInTheDocument();
  });

  it("renders servers with address and protocol", () => {
    renderCard();
    expect(screen.getByText("Cloudflare")).toBeInTheDocument();
    expect(screen.getByText("9.9.9.9:853")).toBeInTheDocument();
    expect(screen.getByText("TLS")).toBeInTheDocument();
  });

  it("shows the fallback-only note in recursive mode", () => {
    renderCard({ fallbackOnly: true });
    expect(
      screen.getByText(/Recursive resolution is active/),
    ).toBeInTheDocument();
  });

  it("shows the selected mode's behaviour description", () => {
    renderCard({ mode: "fastest" });
    expect(screen.getByTestId("upstream-mode-desc").textContent ?? "").toMatch(
      /fastest/i,
    );
  });

  it("shows per-server radios only in single-server mode", () => {
    renderCard({ mode: "single", selectedUpstream: "1.1.1.1" });
    const radios = screen.getAllByTestId("upstream-select");
    expect(radios).toHaveLength(servers.length);
  });

  it("moves a server down via the row menu (failover mode)", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const { onUpdate } = renderCard();
    const menus = screen.getAllByTestId("upstream-row-menu");
    await user.click(menus[0]);
    await user.click(
      await screen.findByRole("menuitem", { name: "Move down" }),
    );
    expect(onUpdate).toHaveBeenCalledWith([servers[1], servers[0]]);
  });

  it("ignores moving the first server up (out of bounds)", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const { onUpdate } = renderCard();
    const menus = screen.getAllByTestId("upstream-row-menu");
    await user.click(menus[0]);
    await user.click(await screen.findByRole("menuitem", { name: "Move up" }));
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("removes a server via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const { onUpdate } = renderCard();
    const menus = screen.getAllByTestId("upstream-row-menu");
    await user.click(menus[1]);
    await user.click(await screen.findByTestId("upstream-remove"));
    expect(onUpdate).toHaveBeenCalledWith([servers[0]]);
  });

  it("adds a plain UDP server through the inline form", async () => {
    const user = userEvent.setup();
    const { onUpdate } = renderCard({ servers: [] });
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
    const { onUpdate } = renderCard({ servers: [] });
    await user.click(screen.getByTestId("upstream-add"));
    // Pick the TLS protocol to reveal the SNI field. Target the protocol
    // select by its test id (the Routing dropdown is also a combobox).
    await user.click(screen.getByTestId("upstream-protocol"));
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

  it("selecting a server radio in single mode calls onSelectServer", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const { onSelectServer } = renderCard({
      mode: "single",
      selectedUpstream: "1.1.1.1",
    });
    const radios = screen.getAllByTestId("upstream-select");
    await user.click(radios[1]); // pick Quad9
    expect(onSelectServer).toHaveBeenCalledWith("9.9.9.9");
  });

  it("choosing Fastest from the routing dropdown calls onModeChange", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const { onModeChange } = renderCard();
    await user.click(screen.getByTestId("upstream-mode"));
    await user.click(
      await screen.findByRole("option", { name: "Fastest response" }),
    );
    expect(onModeChange).toHaveBeenCalledWith("fastest");
  });

  it("choosing Single from the dropdown auto-selects the first server", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const { onSelectServer } = renderCard();
    await user.click(screen.getByTestId("upstream-mode"));
    await user.click(
      await screen.findByRole("option", { name: "Single server" }),
    );
    expect(onSelectServer).toHaveBeenCalledWith("1.1.1.1");
  });

  it("cancelling the add form hides it", async () => {
    const user = userEvent.setup();
    renderCard({ servers: [] });
    await user.click(screen.getByTestId("upstream-add"));
    expect(screen.getByTestId("upstream-name")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("upstream-name")).not.toBeInTheDocument();
  });
});

import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceSelect } from "@/components/compound/DeviceSelect";
import { makeDevice, renderWithProviders } from "../../test-utils";

// Radix Select relies on a few DOM APIs jsdom doesn't implement.
beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
  Element.prototype.setPointerCapture = vi.fn();
  window.HTMLElement.prototype.scrollIntoView = vi.fn();
});

describe("DeviceSelect", () => {
  it("shows the any-device label when no device is selected", () => {
    renderWithProviders(
      <DeviceSelect devices={[]} value="" onChange={vi.fn()} />,
    );
    expect(screen.getByText("Any device")).toBeInTheDocument();
  });

  it("renders a custom any-label", () => {
    renderWithProviders(
      <DeviceSelect
        devices={[]}
        value=""
        onChange={vi.fn()}
        anyLabel="All hosts"
      />,
    );
    expect(screen.getByText("All hosts")).toBeInTheDocument();
  });

  it("shows the selected device's name in the trigger", () => {
    renderWithProviders(
      <DeviceSelect
        devices={[makeDevice({ id: "d1", name: "TV", last_ip: "10.0.0.5" })]}
        value="10.0.0.5"
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByText("TV")).toBeInTheDocument();
  });

  it("lists devices and reports the chosen IP", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <DeviceSelect
        devices={[
          makeDevice({ id: "d1", name: "TV", last_ip: "10.0.0.5" }),
          makeDevice({
            id: "d2",
            name: null,
            hostname: null,
            last_ip: "10.0.0.6",
          }),
        ]}
        value=""
        onChange={onChange}
        id="dev-select"
      />,
    );
    await user.click(screen.getByRole("combobox"));
    const options = await screen.findAllByRole("option");
    // "Any device" + two devices.
    expect(options.length).toBe(3);
    await user.click(screen.getByRole("option", { name: /TV/ }));
    expect(onChange).toHaveBeenCalledWith("10.0.0.5");
  });

  it("maps the any option back to the empty sentinel", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <DeviceSelect
        devices={[makeDevice({ id: "d1", name: "TV", last_ip: "10.0.0.5" })]}
        value="10.0.0.5"
        onChange={onChange}
      />,
    );
    await user.click(screen.getByRole("combobox"));
    const listbox = await screen.findByRole("listbox");
    await user.click(within(listbox).getByText("Any device"));
    expect(onChange).toHaveBeenCalledWith("");
  });

  // Order is the component's job, not the caller's — asserted on the rendered
  // options so reverting the sort can't pass silently.
  it("lists devices alphabetically regardless of the order passed in", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSelect
        devices={[
          makeDevice({ id: "d1", name: "zeta", last_ip: "10.0.0.1" }),
          makeDevice({ id: "d2", name: "Alpha", last_ip: "10.0.0.2" }),
          makeDevice({ id: "d3", hostname: "beta", last_ip: "10.0.0.3" }),
        ]}
        value=""
        onChange={vi.fn()}
        includeAny={false}
      />,
    );
    await user.click(screen.getByRole("combobox"));
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining("Alpha"),
      expect.stringContaining("beta"),
      expect.stringContaining("zeta"),
    ]);
  });

  // Radix reserves "" for "no selection" and throws on a SelectItem carrying
  // it, which would unmount the whole dropdown — not just the offending row.
  // A device with no IP is also exactly what an IP filter can never match, so
  // omitting it loses nothing.
  it("omits a device with a blank key instead of crashing the dropdown", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSelect
        devices={[
          makeDevice({ id: "d1", name: "Has IP", last_ip: "10.0.0.9" }),
          makeDevice({ id: "d2", name: "No IP", last_ip: "" }),
        ]}
        value=""
        onChange={vi.fn()}
        includeAny={false}
      />,
    );
    await user.click(screen.getByRole("combobox"));
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining("Has IP"),
    ]);
  });

  it("shows the empty label when every device lacks a usable key", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSelect
        devices={[makeDevice({ id: "d1", name: "No IP", last_ip: "" })]}
        value=""
        onChange={vi.fn()}
        includeAny={false}
        emptyLabel="No selectable devices."
      />,
    );
    await user.click(screen.getByRole("combobox"));
    expect(
      await screen.findByText("No selectable devices."),
    ).toBeInTheDocument();
    expect(screen.queryAllByRole("option")).toHaveLength(0);
  });

  // An unnamed device must resolve its label through the shared chain, so the
  // dropdown and the devices table agree on where it belongs. Keyed by id
  // because the IP-keyed mode drops a device with a blank IP (see above).
  it("labels an unnamed device by IP, and a device with no IP by MAC", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceSelect
        devices={[
          makeDevice({ id: "d1", last_ip: "10.0.0.9" }),
          makeDevice({
            id: "d2",
            name: "  ",
            hostname: "",
            last_ip: "",
            mac: "AA:BB",
          }),
        ]}
        valueKey="id"
        value=""
        onChange={vi.fn()}
        includeAny={false}
      />,
    );
    await user.click(screen.getByRole("combobox"));
    const options = await screen.findAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining("10.0.0.9"),
      expect.stringContaining("AA:BB"),
    ]);
  });
});

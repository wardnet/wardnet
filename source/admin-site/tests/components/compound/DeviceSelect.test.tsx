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
});

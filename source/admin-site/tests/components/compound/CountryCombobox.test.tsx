import { describe, expect, it, vi, beforeAll } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { CountryInfo } from "@wardnet/js";

// jsdom lacks ResizeObserver, which cmdk (the Combobox list) uses.
beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as never;
  // cmdk scrolls the active item into view; jsdom has no layout.
  Element.prototype.scrollIntoView ??= () => {};
});
import { CountryCombobox } from "@/components/compound/CountryCombobox";
import { renderWithProviders } from "../../test-utils";

const countries: CountryInfo[] = [
  { code: "us", name: "United States" } as CountryInfo,
  { code: "de", name: "Germany" } as CountryInfo,
];

describe("CountryCombobox", () => {
  it("shows the placeholder when nothing is selected", () => {
    renderWithProviders(
      <CountryCombobox countries={countries} value="" onChange={vi.fn()} />,
    );
    expect(screen.getByText("All countries")).toBeInTheDocument();
  });

  it("uses a custom placeholder", () => {
    renderWithProviders(
      <CountryCombobox
        countries={countries}
        value=""
        onChange={vi.fn()}
        placeholder="Pick one"
      />,
    );
    expect(screen.getByText("Pick one")).toBeInTheDocument();
  });

  it("shows the selected country name in the trigger", () => {
    renderWithProviders(
      <CountryCombobox countries={countries} value="de" onChange={vi.fn()} />,
    );
    expect(screen.getByText("Germany")).toBeInTheDocument();
  });

  it("opens the list and selects a country", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <CountryCombobox countries={countries} value="" onChange={onChange} />,
    );
    await user.click(screen.getByRole("combobox"));
    await user.click(await screen.findByText("United States"));
    expect(onChange).toHaveBeenCalledWith("us");
  });

  it("clears the selection when the current value is chosen again", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <CountryCombobox countries={countries} value="us" onChange={onChange} />,
    );
    await user.click(screen.getByRole("combobox"));
    await user.click(
      await screen.findByRole("option", { name: /United States/ }),
    );
    expect(onChange).toHaveBeenCalledWith("");
  });
});

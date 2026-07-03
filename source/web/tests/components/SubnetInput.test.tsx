import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SubnetInput } from "../../src/components/SubnetInput";

// Radix Select needs these DOM APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};

const onChange = vi.fn();

function baseOctets(): HTMLInputElement[] {
  return within(screen.getByTestId("subnet-ip")).getAllByRole(
    "textbox",
  ) as HTMLInputElement[];
}

/** Paste a full IPv4 into the base-address input (fills all octets). */
async function setBase(user: ReturnType<typeof userEvent.setup>, ip: string) {
  await user.click(baseOctets()[0]);
  await user.paste(ip);
}

beforeEach(() => onChange.mockReset());

describe("SubnetInput", () => {
  it("picks the prefix from the size dropdown", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<SubnetInput value="" onChange={onChange} />);
    await setBase(user, "10.44.0.0");
    await user.click(screen.getByTestId("subnet-size"));
    await user.click(
      await screen.findByRole("option", { name: /Up to 62 devices/ }),
    );
    expect(onChange).toHaveBeenLastCalledWith("10.44.0.0/26");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent("10.44.0.0/26");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent("62 usable");
  });

  it("locks the host octet to 0 for a /24", () => {
    render(<SubnetInput value="10.44.0.0/24" onChange={onChange} />);
    const octets = baseOctets();
    // /24 → the last octet is entirely host bits: read-only, shown as 0.
    expect(octets[3]).toHaveAttribute("readonly");
    expect(octets[3].value).toBe("0");
    expect(octets[2]).not.toHaveAttribute("readonly");
  });

  it("accepts an explicit prefix in advanced mode", async () => {
    const user = userEvent.setup();
    render(<SubnetInput value="" onChange={onChange} />);
    await setBase(user, "192.168.1.0");
    await user.click(screen.getByTestId("subnet-mode-toggle"));
    await user.clear(screen.getByTestId("subnet-prefix"));
    await user.type(screen.getByTestId("subnet-prefix"), "24");
    expect(onChange).toHaveBeenLastCalledWith("192.168.1.0/24");
  });

  it("initializes from an existing CIDR value", () => {
    render(<SubnetInput value="10.9.0.0/16" onChange={onChange} />);
    const octets = baseOctets();
    expect(octets[0].value).toBe("10");
    expect(octets[1].value).toBe("9");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent("10.9.0.0/16");
  });

  it("rejects a non-private (public) range", async () => {
    const user = userEvent.setup();
    render(<SubnetInput value="" onChange={onChange} />);
    await setBase(user, "8.8.0.0");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent(
      /Must be a private range/i,
    );
    expect(onChange).toHaveBeenLastCalledWith("");
  });

  it("emits empty while the address is incomplete", async () => {
    const user = userEvent.setup();
    render(<SubnetInput value="" onChange={onChange} />);
    await user.click(baseOctets()[0]);
    await user.keyboard("10");
    expect(onChange).toHaveBeenLastCalledWith("");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent(
      /Enter a base address/i,
    );
  });
});

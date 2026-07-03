import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SubnetInput } from "../../src/components/SubnetInput";

const onChange = vi.fn();

async function fillOctets(
  user: ReturnType<typeof userEvent.setup>,
  ...vals: number[]
) {
  for (let i = 0; i < vals.length; i++) {
    await user.clear(screen.getByTestId(`subnet-octet-${i}`));
    await user.type(screen.getByTestId(`subnet-octet-${i}`), String(vals[i]));
  }
}

beforeEach(() => onChange.mockReset());

describe("SubnetInput", () => {
  it("derives the prefix from a device count in simple mode", async () => {
    const user = userEvent.setup();
    render(<SubnetInput value="" onChange={onChange} />);
    await fillOctets(user, 10, 44, 0, 0);
    await user.clear(screen.getByTestId("subnet-devices"));
    await user.type(screen.getByTestId("subnet-devices"), "50");
    // 50 devices → /26 (62 usable), network-masked.
    expect(onChange).toHaveBeenLastCalledWith("10.44.0.0/26");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent("10.44.0.0/26");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent("62 usable");
  });

  it("masks host bits to the network address", async () => {
    const user = userEvent.setup();
    render(<SubnetInput value="" onChange={onChange} />);
    await fillOctets(user, 10, 44, 0, 37);
    await user.clear(screen.getByTestId("subnet-devices"));
    await user.type(screen.getByTestId("subnet-devices"), "50");
    expect(onChange).toHaveBeenLastCalledWith("10.44.0.0/26");
  });

  it("accepts an explicit prefix in advanced mode", async () => {
    const user = userEvent.setup();
    render(<SubnetInput value="" onChange={onChange} />);
    await fillOctets(user, 192, 168, 1, 0);
    await user.click(screen.getByTestId("subnet-mode-advanced"));
    await user.clear(screen.getByTestId("subnet-prefix"));
    await user.type(screen.getByTestId("subnet-prefix"), "24");
    expect(onChange).toHaveBeenLastCalledWith("192.168.1.0/24");
  });

  it("initializes from an existing CIDR value", () => {
    render(<SubnetInput value="10.9.0.0/16" onChange={onChange} />);
    expect(screen.getByTestId("subnet-octet-0")).toHaveValue("10");
    expect(screen.getByTestId("subnet-octet-1")).toHaveValue("9");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent("10.9.0.0/16");
  });

  it("emits empty while the address is incomplete", async () => {
    const user = userEvent.setup();
    render(<SubnetInput value="" onChange={onChange} />);
    await user.type(screen.getByTestId("subnet-octet-0"), "10");
    expect(onChange).toHaveBeenLastCalledWith("");
    expect(screen.getByTestId("subnet-cidr")).toHaveTextContent(
      /Enter a base address/i,
    );
  });
});

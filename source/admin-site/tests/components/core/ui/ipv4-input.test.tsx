import { useState } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Ipv4Input } from "@/components/core/ui/ipv4-input";

function Harness({ initial = "" }: { initial?: string }) {
  const [value, setValue] = useState(initial);
  return <Ipv4Input value={value} onChange={setValue} data-testid="ip" />;
}

function octets() {
  return within(screen.getByTestId("ip")).getAllByRole(
    "textbox",
  ) as HTMLInputElement[];
}

describe("Ipv4Input", () => {
  it("renders four octets from the value", () => {
    render(<Ipv4Input value="10.0.0.1" onChange={vi.fn()} data-testid="ip" />);
    const inputs = octets();
    expect(inputs).toHaveLength(4);
    expect(inputs[0].value).toBe("10");
    expect(inputs[3].value).toBe("1");
  });

  it("clamps octets to 255 and strips non-digits", () => {
    render(<Harness />);
    const inputs = octets();
    fireEvent.change(inputs[0], { target: { value: "3a0b0" } });
    // Non-digits stripped -> "300", clamped to 255.
    expect(inputs[0].value).toBe("255");
  });

  it("auto-tabs after 3 digits or when > 25", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const inputs = octets();
    await user.click(inputs[0]);
    await user.keyboard("192");
    expect(document.activeElement).toBe(inputs[1]);
    await user.keyboard("30"); // >25 triggers advance
    expect(document.activeElement).toBe(inputs[2]);
  });

  it("advances on '.' and navigates back with backspace", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const inputs = octets();
    await user.click(inputs[0]);
    await user.keyboard("1");
    await user.keyboard(".");
    expect(document.activeElement).toBe(inputs[1]);
    await user.keyboard("{Backspace}");
    expect(document.activeElement).toBe(inputs[0]);
  });

  it("arrow keys move between octets", async () => {
    const user = userEvent.setup();
    render(<Harness initial="10.20.30.40" />);
    const inputs = octets();
    await user.click(inputs[1]);
    inputs[1].setSelectionRange(0, 0);
    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(inputs[0]);
  });

  it("pastes a full IPv4 address and clamps", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const inputs = octets();
    await user.click(inputs[0]);
    await user.paste("999.1.2.3");
    expect(inputs[0].value).toBe("255");
    expect(inputs[3].value).toBe("3");
  });
});

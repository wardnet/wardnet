import { useState } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MacInput } from "@/components/core/ui/mac-input";

function Harness({ initial = "" }: { initial?: string }) {
  const [value, setValue] = useState(initial);
  return <MacInput value={value} onChange={setValue} data-testid="mac" />;
}

function segs() {
  return within(screen.getByTestId("mac")).getAllByRole(
    "textbox",
  ) as HTMLInputElement[];
}

describe("MacInput", () => {
  it("renders six uppercase segments from the value", () => {
    render(
      <MacInput
        value="aa:bb:cc:dd:ee:ff"
        onChange={vi.fn()}
        data-testid="mac"
      />,
    );
    const inputs = segs();
    expect(inputs).toHaveLength(6);
    expect(inputs[0].value).toBe("AA");
    expect(inputs[5].value).toBe("FF");
  });

  it("types across segments with auto-tab", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const inputs = segs();
    await user.click(inputs[0]);
    await user.keyboard("aabbccddeeff");
    // Auto-tab fills subsequent segments as pairs complete.
    expect(inputs[0].value).toBe("AA");
    expect(inputs[1].value).toBe("BB");
    expect(inputs[5].value).toBe("FF");
  });

  it("advances on ':' separator and clears via backspace navigation", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const inputs = segs();
    await user.click(inputs[0]);
    await user.keyboard("aa");
    // now on segment 1
    await user.keyboard(":");
    await user.keyboard("bb");
    expect(inputs[1].value).toBe("BB");
    // backspace from an empty segment 3 moves back to 2
    await user.click(inputs[3]);
    await user.keyboard("{Backspace}");
    expect(document.activeElement).toBe(inputs[2]);
  });

  it("navigates with arrow keys", async () => {
    const user = userEvent.setup();
    render(<Harness initial="aa:bb:cc:dd:ee:ff" />);
    const inputs = segs();
    await user.click(inputs[2]);
    inputs[2].setSelectionRange(0, 0);
    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(inputs[1]);
    inputs[1].setSelectionRange(inputs[1].value.length, inputs[1].value.length);
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(inputs[2]);
  });

  it("handles a pasted full MAC in any format", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const inputs = segs();
    await user.click(inputs[0]);
    await user.paste("11-22-33-44-55-66");
    expect(inputs[0].value).toBe("11");
    expect(inputs[5].value).toBe("66");
  });
});

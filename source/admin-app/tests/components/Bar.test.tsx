import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Bar } from "@/components/Bar";

function fill(container: HTMLElement): HTMLElement {
  // The inner (filled) div is the last child of the track.
  const track = container.firstElementChild as HTMLElement;
  return track.firstElementChild as HTMLElement;
}

describe("Bar", () => {
  it("clamps percent above 100 and below 0", () => {
    const { container: over } = render(<Bar percent={150} />);
    expect(fill(over).style.width).toBe("100%");

    const { container: under } = render(<Bar percent={-20} />);
    expect(fill(under).style.width).toBe("0%");
  });

  it("usage variant: danger ≥90, warn ≥70, accent below", () => {
    expect(fill(render(<Bar percent={95} />).container).className).toContain("bg-danger");
    expect(fill(render(<Bar percent={75} />).container).className).toContain("bg-warn");
    expect(fill(render(<Bar percent={40} />).container).className).toContain("bg-accent");
  });

  it("rate variant: accent ≥80, warn ≥50, danger below", () => {
    expect(fill(render(<Bar percent={85} variant="rate" />).container).className).toContain("bg-accent");
    expect(fill(render(<Bar percent={60} variant="rate" />).container).className).toContain("bg-warn");
    expect(fill(render(<Bar percent={20} variant="rate" />).container).className).toContain("bg-danger");
  });
});

import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { BellIcon } from "lucide-react";

import { Placeholder } from "../../src/pages/Placeholder";

describe("Placeholder", () => {
  it("renders the title, description and a coming-soon badge", () => {
    render(
      <Placeholder
        title="Notifications"
        description="Push coming later"
        Icon={BellIcon}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "Notifications" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Push coming later")).toBeInTheDocument();
    expect(screen.getByText("Coming soon")).toBeInTheDocument();
  });
});

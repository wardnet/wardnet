import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";

import { Legal } from "@/pages/Legal";

describe("Legal", () => {
  it("renders the Terms of Service content for slug=terms", () => {
    render(
      <MemoryRouter>
        <Legal slug="terms" />
      </MemoryRouter>,
    );
    expect(screen.getByRole("heading", { level: 1, name: "Terms of Service" })).toBeInTheDocument();
  });

  it("renders the Privacy Policy content for slug=privacy", () => {
    render(
      <MemoryRouter>
        <Legal slug="privacy" />
      </MemoryRouter>,
    );
    expect(screen.getByRole("heading", { level: 1, name: "Privacy Policy" })).toBeInTheDocument();
  });

  it("renders the back button in the Navbar", () => {
    render(
      <MemoryRouter>
        <Legal slug="terms" />
      </MemoryRouter>,
    );
    expect(screen.getByRole("button", { name: /go back/i })).toBeInTheDocument();
  });
});

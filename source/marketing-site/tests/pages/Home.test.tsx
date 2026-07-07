import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { describe, expect, it } from "vitest";
import { Home } from "@/pages/Home";

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname + location.search}</div>;
}

function renderHome(initialEntries = ["/"]) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <Home />
      <LocationDisplay />
    </MemoryRouter>,
  );
}

describe("Home", () => {
  it("initially shows the hero, not the content", () => {
    renderHome();
    expect(screen.getByRole("heading", { name: "Wardnet" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /scroll to features/i })).toBeInTheDocument();
    expect(
      screen.queryByText("Everything you need to protect your network"),
    ).not.toBeInTheDocument();
  });

  it("fades the hero out, then swaps to content, after clicking Explore", async () => {
    renderHome();

    await userEvent.click(screen.getByRole("button", { name: /scroll to features/i }));

    // Hero is still mounted immediately after the click, mid-fade.
    expect(screen.getByRole("button", { name: /scroll to features/i })).toBeInTheDocument();

    await waitFor(() =>
      expect(screen.getByText("Everything you need to protect your network")).toBeInTheDocument(),
    );
    expect(screen.queryByRole("button", { name: /scroll to features/i })).not.toBeInTheDocument();
  });

  it("skips the hero when the URL requests the content view directly", () => {
    renderHome(["/?view=content"]);
    expect(screen.getByText("Everything you need to protect your network")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /scroll to features/i })).not.toBeInTheDocument();
  });

  it("updates the URL to /?view=content once the content is shown, so a real back navigation lands on content, not the hero", async () => {
    renderHome();

    await userEvent.click(screen.getByRole("button", { name: /scroll to features/i }));
    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent("/?view=content"),
    );
  });
});

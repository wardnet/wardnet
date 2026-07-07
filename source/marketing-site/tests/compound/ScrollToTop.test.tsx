import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MemoryRouter, Routes, Route, Link } from "react-router";
import { ScrollToTop } from "@/components/compound/ScrollToTop";

function Page({ label }: { label: string }) {
  return (
    <div>
      <p>{label}</p>
      <Link to="/other">Go to other page</Link>
    </div>
  );
}

describe("ScrollToTop", () => {
  it("scrolls to top when navigating to a new page", async () => {
    const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
    const user = userEvent.setup();

    render(
      <MemoryRouter initialEntries={["/"]}>
        <ScrollToTop />
        <Routes>
          <Route path="/" element={<Page label="Home" />} />
          <Route path="/other" element={<Page label="Other" />} />
        </Routes>
      </MemoryRouter>,
    );

    scrollTo.mockClear();
    await user.click(screen.getByText("Go to other page"));

    expect(screen.getByText("Other")).toBeInTheDocument();
    expect(scrollTo).toHaveBeenCalledWith({ top: 0, left: 0, behavior: "instant" });

    scrollTo.mockRestore();
  });

  it("does not scroll on the initial render (navigation type is POP)", () => {
    const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});

    render(
      <MemoryRouter initialEntries={["/"]}>
        <ScrollToTop />
        <Routes>
          <Route path="/" element={<Page label="Home" />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(scrollTo).not.toHaveBeenCalled();
    scrollTo.mockRestore();
  });
});

import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { describe, expect, it } from "vitest";
import { AuthLayout } from "@/components/layouts/AuthLayout";

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route element={<AuthLayout />}>
          <Route path={path} element={<div>routed child</div>} />
        </Route>
      </Routes>
    </MemoryRouter>,
  );
}

describe("AuthLayout", () => {
  it("shows the setup tagline on /setup and renders the outlet", () => {
    renderAt("/setup");
    expect(
      screen.getByText("Let's get your network ready"),
    ).toBeInTheDocument();
    expect(screen.getByText("routed child")).toBeInTheDocument();
  });

  it("shows the sign-in tagline elsewhere", () => {
    renderAt("/login");
    expect(
      screen.getByText("Sign in to manage your network"),
    ).toBeInTheDocument();
  });
});

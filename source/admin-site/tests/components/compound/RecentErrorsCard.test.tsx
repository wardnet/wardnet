import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import type { RecentError } from "@wardnet/web";
import { RecentErrorsCard } from "@/components/compound/RecentErrorsCard";
import { renderWithProviders } from "../../test-utils";

describe("RecentErrorsCard", () => {
  it("shows the empty state when there are no errors", () => {
    renderWithProviders(<RecentErrorsCard errors={[]} />);
    expect(screen.getByText("No recent errors")).toBeInTheDocument();
  });

  it("renders each error newest-first with level classes", () => {
    const errors: RecentError[] = [
      {
        level: "WARN",
        timestamp: "2026-01-01T10:00:00.123Z",
        message: "first warning",
      },
      {
        level: "ERROR",
        timestamp: "2026-01-01T11:00:00.000Z",
        message: "second error",
      },
      {
        level: "INFO",
        timestamp: "",
        message: "third info",
      },
    ] as RecentError[];
    const { container } = renderWithProviders(
      <RecentErrorsCard errors={errors} />,
    );

    expect(screen.getByText("first warning")).toBeInTheDocument();
    expect(screen.getByText("second error")).toBeInTheDocument();
    expect(screen.getByText("third info")).toBeInTheDocument();

    const rows = container.querySelectorAll(".logrow");
    // reversed order: INFO first, then ERROR, then WARN
    expect(rows[0].className).toContain("is-info");
    expect(rows[1].className).toContain("is-err");
    expect(rows[2].className).toContain("is-warn");

    // Empty timestamp renders as blank, populated one includes ms.
    const times = container.querySelectorAll(".logrow .t");
    expect(times[0].textContent).toBe("");
    expect(times[1].textContent).toContain(".000");
  });

  it("treats WARNING the same as WARN", () => {
    const errors = [
      { level: "WARNING", timestamp: "", message: "warn-like" },
    ] as RecentError[];
    const { container } = renderWithProviders(
      <RecentErrorsCard errors={errors} />,
    );
    expect(container.querySelector(".logrow")?.className).toContain("is-warn");
  });
});

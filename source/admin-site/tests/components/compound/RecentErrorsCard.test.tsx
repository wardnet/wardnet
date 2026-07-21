import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import type { SystemDiagnostic } from "@wardnet/web";
import { RecentErrorsCard } from "@/components/compound/RecentErrorsCard";
import { renderWithProviders } from "../../test-utils";

function diag(overrides: Partial<SystemDiagnostic>): SystemDiagnostic {
  return {
    timestamp: "",
    code: "tunnel_start_failed",
    severity: "error",
    component: "tunnel",
    message: "something happened",
    hint: "",
    ...overrides,
  };
}

describe("RecentErrorsCard", () => {
  it("shows the empty state when there are no errors", () => {
    renderWithProviders(<RecentErrorsCard errors={[]} />);
    expect(screen.getByText("No recent errors")).toBeInTheDocument();
  });

  it("renders diagnostics newest-first by timestamp with severity classes", () => {
    const errors: SystemDiagnostic[] = [
      diag({
        severity: "warning",
        timestamp: "2026-01-01T10:00:00.123Z",
        message: "first warning",
      }),
      diag({
        severity: "error",
        timestamp: "2026-01-01T11:00:00.000Z",
        message: "second error",
      }),
      diag({ severity: "info", timestamp: "", message: "third info" }),
    ];
    const { container } = renderWithProviders(
      <RecentErrorsCard errors={errors} />,
    );

    expect(screen.getByText("first warning")).toBeInTheDocument();
    expect(screen.getByText("second error")).toBeInTheDocument();
    expect(screen.getByText("third info")).toBeInTheDocument();

    const rows = container.querySelectorAll(".logrow");
    // Sorted by timestamp descending: 11:00 error, 10:00 warning, then the
    // empty-timestamp info row (which sinks to the bottom).
    expect(rows[0].className).toContain("is-err");
    expect(rows[1].className).toContain("is-warn");
    expect(rows[2].className).toContain("is-info");

    // Rows carry the event timestamp; the empty one renders blank.
    const times = container.querySelectorAll(".logrow .t");
    expect(times[0].textContent).toContain(".000");
    expect(times[1].textContent).toContain(".123");
    expect(times[2].textContent).toBe("");
  });

  it("renders the remediation hint when present", () => {
    const errors: SystemDiagnostic[] = [
      diag({ message: "tunnel down", hint: "check the endpoint and keys" }),
    ];
    renderWithProviders(<RecentErrorsCard errors={errors} />);
    expect(screen.getByText("check the endpoint and keys")).toBeInTheDocument();
  });
});

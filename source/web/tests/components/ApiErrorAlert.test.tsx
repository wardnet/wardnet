import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { WardnetApiError } from "@wardnet/js";
import { ApiErrorAlert } from "../../src/components/ApiErrorAlert";

describe("ApiErrorAlert", () => {
  it("shows the message and request id from a WardnetApiError", () => {
    const err = new WardnetApiError(
      500,
      "Server Error",
      { error: "err", detail: "Disk full" },
      "req-9",
    );
    render(<ApiErrorAlert error={err} />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByText("Disk full")).toBeInTheDocument();
    expect(screen.getByText("Request ID: req-9")).toBeInTheDocument();
  });

  it("uses the fallback and omits the request id for a generic error", () => {
    render(<ApiErrorAlert error={{}} fallback="Oops" />);
    expect(screen.getByText("Oops")).toBeInTheDocument();
    expect(screen.queryByText(/Request ID/)).not.toBeInTheDocument();
  });
});

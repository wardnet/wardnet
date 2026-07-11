import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  AvailabilityHint,
  CloudflareFields,
  ProviderOption,
  WardnetFields,
} from "@/components/features/wardnet-enrollment";
import { renderWithProviders } from "../../test-utils";

describe("ProviderOption", () => {
  it("renders label/description and fires onSelect", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    renderWithProviders(
      <ProviderOption
        label="Cloudflare"
        description="Bring your own domain"
        selected={false}
        onSelect={onSelect}
      />,
    );
    expect(screen.getByText("Cloudflare")).toBeInTheDocument();
    expect(screen.getByText("Bring your own domain")).toBeInTheDocument();
    await user.click(screen.getByRole("radio"));
    expect(onSelect).toHaveBeenCalled();
  });
});

describe("CloudflareFields", () => {
  it("lowercases the domain and forwards the token", async () => {
    const user = userEvent.setup();
    const onDomainChange = vi.fn();
    const onTokenChange = vi.fn();
    renderWithProviders(
      <CloudflareFields
        domain=""
        onDomainChange={onDomainChange}
        token=""
        onTokenChange={onTokenChange}
      />,
    );
    await user.type(screen.getByLabelText("Domain"), "A");
    expect(onDomainChange).toHaveBeenCalledWith("a");
    await user.type(screen.getByLabelText("Cloudflare API token"), "t");
    expect(onTokenChange).toHaveBeenCalledWith("t");
  });
});

describe("WardnetFields", () => {
  const noop = vi.fn();
  const props = {
    email: "you@example.com",
    onEmailChange: noop,
    code: "",
    onCodeChange: noop,
    slug: "happy-einstein",
    onSlugChange: noop,
    availability: "unknown" as const,
    onSuggest: noop,
    onChangeEmail: noop,
  };

  it("renders the email step", async () => {
    const user = userEvent.setup();
    const onEmailChange = vi.fn();
    renderWithProviders(
      <WardnetFields
        {...props}
        email=""
        step="email"
        onEmailChange={onEmailChange}
      />,
    );
    await user.type(screen.getByLabelText("Wardnet account email"), "a");
    expect(onEmailChange).toHaveBeenCalledWith("a");
  });

  it("renders the code step and change-email button", async () => {
    const user = userEvent.setup();
    const onChangeEmail = vi.fn();
    renderWithProviders(
      <WardnetFields {...props} step="code" onChangeEmail={onChangeEmail} />,
    );
    expect(
      screen.getByText(/We emailed a one-time code to you@example.com/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Change email" }));
    expect(onChangeEmail).toHaveBeenCalled();
  });

  it("renders the slug step with a suggest button", async () => {
    const user = userEvent.setup();
    const onSuggest = vi.fn();
    renderWithProviders(
      <WardnetFields
        {...props}
        step="slug"
        availability="available"
        onSuggest={onSuggest}
      />,
    );
    expect(
      screen.getByText("✓ happy-einstein is available"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Suggest another" }));
    expect(onSuggest).toHaveBeenCalled();
  });
});

describe("AvailabilityHint", () => {
  it("shows the too-short message for a short slug", () => {
    renderWithProviders(<AvailabilityHint availability="invalid" slug="ab" />);
    expect(screen.getByText("At least 3 characters.")).toBeInTheDocument();
  });

  it("shows the reserved message for a reserved slug", () => {
    renderWithProviders(
      <AvailabilityHint availability="invalid" slug="admin" />,
    );
    expect(
      screen.getByText("This name is reserved - try another."),
    ).toBeInTheDocument();
  });

  it("shows the charset message for an otherwise invalid slug", () => {
    renderWithProviders(
      <AvailabilityHint availability="invalid" slug="Bad Name" />,
    );
    expect(
      screen.getByText("Use lowercase letters, digits, and hyphens only."),
    ).toBeInTheDocument();
  });

  it("shows the checking message", () => {
    renderWithProviders(<AvailabilityHint availability="checking" slug="x" />);
    expect(screen.getByText("Checking availability…")).toBeInTheDocument();
  });

  it("shows the available message", () => {
    renderWithProviders(
      <AvailabilityHint availability="available" slug="cool-name" />,
    );
    expect(screen.getByText("✓ cool-name is available")).toBeInTheDocument();
  });

  it("shows the taken message", () => {
    renderWithProviders(
      <AvailabilityHint availability="taken" slug="cool-name" />,
    );
    expect(
      screen.getByText("cool-name is taken - try another"),
    ).toBeInTheDocument();
  });

  it("shows the error message", () => {
    renderWithProviders(<AvailabilityHint availability="error" slug="x" />);
    expect(screen.getByText(/Couldn't check availability/)).toBeInTheDocument();
  });

  it("renders a placeholder for the idle default", () => {
    const { container } = renderWithProviders(
      <AvailabilityHint availability="unknown" slug="x" />,
    );
    expect(container.querySelector("span")).toBeInTheDocument();
  });
});

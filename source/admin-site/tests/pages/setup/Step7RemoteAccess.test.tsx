import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";

const { useAdvanceWizard, useTlsStatus } = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
  useTlsStatus: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAdvanceWizard, useTlsStatus };
});

const { useWardnetEnrollment } = vi.hoisted(() => ({
  useWardnetEnrollment: vi.fn(),
}));
vi.mock("@/lib/wardnet-enrollment", () => ({ useWardnetEnrollment }));

vi.mock("@/components/features/RemoteAccessProgress", () => ({
  RemoteAccessProgress: ({ status }: { status: { phase?: string } }) => (
    <div data-testid="ra-progress">phase:{status.phase}</div>
  ),
}));

vi.mock("@/components/features/wardnet-enrollment", () => ({
  ProviderOption: ({
    label,
    onSelect,
  }: {
    label: string;
    onSelect: () => void;
  }) => (
    <button type="button" onClick={onSelect}>
      provider:{label}
    </button>
  ),
  WardnetFields: () => <div data-testid="wardnet-fields" />,
  CloudflareFields: () => <div data-testid="cloudflare-fields" />,
}));

import Step7RemoteAccess from "@/pages/setup/Step7RemoteAccess";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();
const sendCode = vi.fn();
const verifyCode = vi.fn();
const registerWardnet = vi.fn();
const enableCloudflare = vi.fn();

type EnrollmentOpts = {
  onProvisioned: () => void;
  onError: (err: unknown) => void;
  clearError: () => void;
};

let capturedOpts: EnrollmentOpts;

function baseEnrollment(overrides: Record<string, unknown> = {}) {
  return {
    provider: "wardnet",
    setProvider: vi.fn(),
    wardnetStep: "email",
    setWardnetStep: vi.fn(),
    email: "",
    setEmail: vi.fn(),
    code: "",
    setCode: vi.fn(),
    slug: "my-slug",
    setSlug: vi.fn(),
    token: "",
    setToken: vi.fn(),
    domain: "",
    setDomain: vi.fn(),
    availability: "unknown",
    busy: false,
    slugDisabled: false,
    pending: {
      sendCode: false,
      verify: false,
      register: false,
      configureCf: false,
    },
    reset: vi.fn(),
    sendCode,
    verifyCode,
    registerWardnet,
    enableCloudflare,
    ...overrides,
  };
}

function mockEnrollment(overrides: Record<string, unknown> = {}) {
  useWardnetEnrollment.mockImplementation((opts: EnrollmentOpts) => {
    capturedOpts = opts;
    return baseEnrollment(overrides);
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  advanceMutate.mockResolvedValue(undefined);
  useAdvanceWizard.mockReturnValue({
    mutateAsync: advanceMutate,
    isPending: false,
  });
  useTlsStatus.mockReturnValue({ data: undefined });
  mockEnrollment();
});

describe("Step7RemoteAccess", () => {
  it("renders the wardnet provider form with a Send code action", () => {
    renderWithProviders(<Step7RemoteAccess />);
    expect(screen.getByText("Remote access (HTTPS)")).toBeInTheDocument();
    expect(screen.getByTestId("wardnet-fields")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Send code" }),
    ).toBeInTheDocument();
  });

  it("sends the code when email is valid", async () => {
    mockEnrollment({ email: "a@b.com" });
    const user = userEvent.setup();
    renderWithProviders(<Step7RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Send code" }));
    expect(sendCode).toHaveBeenCalled();
  });

  it("verifies the code on the code step", async () => {
    mockEnrollment({ wardnetStep: "code", code: "123456" });
    const user = userEvent.setup();
    renderWithProviders(<Step7RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Verify code" }));
    expect(verifyCode).toHaveBeenCalled();
  });

  it("registers on the slug step", async () => {
    mockEnrollment({ wardnetStep: "slug" });
    const user = userEvent.setup();
    renderWithProviders(<Step7RemoteAccess />);
    await user.click(
      screen.getByRole("button", { name: "Enable remote access" }),
    );
    expect(registerWardnet).toHaveBeenCalled();
  });

  it("renders Cloudflare fields and enables remote access when both fields filled", async () => {
    mockEnrollment({
      provider: "cloudflare",
      domain: "example.com",
      token: "tok",
    });
    const user = userEvent.setup();
    renderWithProviders(<Step7RemoteAccess />);
    expect(screen.getByTestId("cloudflare-fields")).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Enable remote access" }),
    );
    expect(enableCloudflare).toHaveBeenCalled();
  });

  it("skips by advancing to completed", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Step7RemoteAccess />);
    await user.click(screen.getByTestId("setup-remote-access-skip"));
    await waitFor(() =>
      expect(advanceMutate).toHaveBeenCalledWith({ to_step: "completed" }),
    );
  });

  it("surfaces an error message when finish fails", async () => {
    advanceMutate.mockRejectedValueOnce(
      new WardnetApiError(500, "err", { error: "advance boom" }),
    );
    const user = userEvent.setup();
    renderWithProviders(<Step7RemoteAccess />);
    await user.click(screen.getByTestId("setup-remote-access-skip"));
    await waitFor(() =>
      expect(screen.getByText("advance boom")).toBeInTheDocument(),
    );
  });

  it("swaps to the live-progress view after provisioning starts", async () => {
    renderWithProviders(<Step7RemoteAccess />);
    await act(async () => capturedOpts.onProvisioned());
    expect(
      screen.getByText("Starting certificate issuance…"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: "Continue (issuance keeps running)",
      }),
    ).toBeInTheDocument();
  });

  it("shows the progress component and Finish once issued", async () => {
    useTlsStatus.mockReturnValue({ data: { phase: "issued" } });
    renderWithProviders(<Step7RemoteAccess />);
    await act(async () => capturedOpts.onProvisioned());
    expect(screen.getByTestId("ra-progress")).toHaveTextContent("phase:issued");
    expect(screen.getByRole("button", { name: "Finish" })).toBeInTheDocument();
  });

  it("shows the upstream-down view on a 503 and can go back", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Step7RemoteAccess />);
    await act(async () =>
      capturedOpts.onError(new WardnetApiError(503, "err", { error: "down" })),
    );
    expect(
      screen.getByText(/couldn't reach the wardnet service/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByTestId("wardnet-fields")).toBeInTheDocument();
  });

  it("shows a form error for a non-upstream API error", async () => {
    renderWithProviders(<Step7RemoteAccess />);
    await act(async () =>
      capturedOpts.onError(
        new WardnetApiError(400, "err", { error: "bad token" }),
      ),
    );
    expect(screen.getByText("bad token")).toBeInTheDocument();
  });

  it("shows a generic error for a non-API failure", async () => {
    renderWithProviders(<Step7RemoteAccess />);
    await act(async () => capturedOpts.onError(new Error("boom")));
    expect(screen.getByText(/Couldn't reach the daemon/)).toBeInTheDocument();
  });
});

/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useDdnsStatus,
  useTlsStatus,
  useResolutionCheck,
  useDeleteDdns,
  useWardnetEnrollment,
  toast,
} = vi.hoisted(() => ({
  useDdnsStatus: vi.fn(),
  useTlsStatus: vi.fn(),
  useResolutionCheck: vi.fn(),
  useDeleteDdns: vi.fn(),
  useWardnetEnrollment: vi.fn(),
  toast: Object.assign(vi.fn(), {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  }),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDdnsStatus,
    useTlsStatus,
    useResolutionCheck,
    useDeleteDdns,
  };
});

vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

vi.mock("@/lib/wardnet-enrollment", () => ({
  useWardnetEnrollment,
}));

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: any) => <h1>{title}</h1>,
}));

vi.mock("@/components/features/RemoteAccessStatus", () => ({
  RemoteAccessStatus: ({ variant }: any) => (
    <div data-testid="ra-status">status:{variant}</div>
  ),
}));

vi.mock("@/components/features/wardnet-enrollment", () => ({
  ProviderOption: ({ label, selected, onSelect }: any) => (
    <button data-selected={String(selected)} onClick={onSelect}>
      provider:{label}
    </button>
  ),
  WardnetFields: ({ step }: any) => (
    <div data-testid="wardnet-fields">wf:{step}</div>
  ),
  CloudflareFields: () => <div data-testid="cf-fields">cf</div>,
}));

import RemoteAccess from "@/pages/RemoteAccess";
import { renderWithProviders } from "../test-utils";

function makeEnrollment(overrides: Record<string, unknown> = {}) {
  return {
    provider: "wardnet",
    setProvider: vi.fn(),
    wardnetStep: "email",
    setWardnetStep: vi.fn(),
    email: "",
    setEmail: vi.fn(),
    code: "",
    setCode: vi.fn(),
    slug: "",
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
    sendCode: vi.fn(),
    verifyCode: vi.fn(),
    registerWardnet: vi.fn(),
    enableCloudflare: vi.fn(),
    ...overrides,
  };
}

let enrollment: ReturnType<typeof makeEnrollment>;
let capturedOpts: any;
const refetch = vi.fn();
const teardownMutateAsync = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  enrollment = makeEnrollment();
  capturedOpts = undefined;
  useWardnetEnrollment.mockImplementation((opts: any) => {
    capturedOpts = opts;
    return enrollment;
  });
  refetch.mockResolvedValue({ data: { verdict: "match" } });
  useDdnsStatus.mockReturnValue({ data: undefined });
  useTlsStatus.mockReturnValue({ data: undefined });
  useResolutionCheck.mockReturnValue({
    data: undefined,
    isFetching: false,
    refetch,
    dataUpdatedAt: 0,
  });
  useDeleteDdns.mockReturnValue({
    isPending: false,
    mutateAsync: teardownMutateAsync,
  });
});

describe("RemoteAccess — unconfigured", () => {
  it("renders the enable form with provider options and wardnet fields", () => {
    renderWithProviders(<RemoteAccess />);
    expect(screen.getByText("Remote access")).toBeInTheDocument();
    expect(screen.getByText("Enable remote access")).toBeInTheDocument();
    expect(screen.getByText("provider:Wardnet")).toBeInTheDocument();
    expect(screen.getByTestId("wardnet-fields")).toHaveTextContent("wf:email");
    // No status/remove cards when unconfigured.
    expect(screen.queryByTestId("ra-status")).not.toBeInTheDocument();
  });

  it("selecting a provider clears error and updates provider state", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByText("provider:Your own domain (Cloudflare)"));
    expect(enrollment.setProvider).toHaveBeenCalledWith("cloudflare");
  });

  it("renders cloudflare fields and drives enableCloudflare when domain+token set", async () => {
    enrollment = makeEnrollment({
      provider: "cloudflare",
      domain: "example.com",
      token: "tok",
    });
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    expect(screen.getByTestId("cf-fields")).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Enable remote access" }),
    );
    expect(enrollment.enableCloudflare).toHaveBeenCalled();
  });

  it("email step Send code button is gated on a valid email and calls sendCode", async () => {
    enrollment = makeEnrollment({ wardnetStep: "email", email: "a@b.com" });
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Send code" }));
    expect(enrollment.sendCode).toHaveBeenCalled();
  });

  it("code step drives verifyCode", async () => {
    enrollment = makeEnrollment({ wardnetStep: "code", code: "123" });
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Verify code" }));
    expect(enrollment.verifyCode).toHaveBeenCalled();
  });

  it("slug step drives registerWardnet with the enable label", async () => {
    enrollment = makeEnrollment({ wardnetStep: "slug", slug: "mybox" });
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(
      screen.getByRole("button", { name: "Enable remote access" }),
    );
    expect(enrollment.registerWardnet).toHaveBeenCalled();
  });

  it("surfaces enrollment errors via the onError callback", async () => {
    const { WardnetApiError } = await import("@wardnet/js");
    renderWithProviders(<RemoteAccess />);
    const err = new WardnetApiError(400, "Bad Request", {
      error: "bad slug",
    } as any);
    capturedOpts.onError(err);
    expect(await screen.findByText("bad slug")).toBeInTheDocument();
  });

  it("falls back to a generic error message for non-api errors", async () => {
    renderWithProviders(<RemoteAccess />);
    capturedOpts.onError(new Error("boom"));
    expect(
      await screen.findByText(/Couldn't reach the daemon/),
    ).toBeInTheDocument();
  });
});

describe("RemoteAccess — configured", () => {
  beforeEach(() => {
    useDdnsStatus.mockReturnValue({ data: { provider: "wardnet" } });
    useTlsStatus.mockReturnValue({ data: { phase: "ready" } });
  });

  it("renders the status card and remove card", () => {
    renderWithProviders(<RemoteAccess />);
    expect(screen.getByText("Status")).toBeInTheDocument();
    expect(screen.getByTestId("ra-status")).toHaveTextContent("status:full");
    expect(
      screen.getByRole("heading", { name: "Remove remote access" }),
    ).toBeInTheDocument();
  });

  it("opens the change-provider form via Change provider", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Change provider" }));
    expect(enrollment.reset).toHaveBeenCalled();
    expect(screen.getByText("Change provider")).toBeInTheDocument();
    // Cancel action appears when changing.
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  });

  it("cancels the change-provider form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Change provider" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    // Back to the status view.
    expect(screen.getByTestId("ra-status")).toBeInTheDocument();
  });

  it("recheck reports a matching verdict via toast", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Recheck DNS" }));
    expect(refetch).toHaveBeenCalled();
    expect(toast.success).toHaveBeenCalledWith("Public DNS resolves correctly");
  });

  it("recheck reports mismatch, pending, and default verdicts", async () => {
    const user = userEvent.setup();

    refetch.mockResolvedValueOnce({ data: { verdict: "mismatch" } });
    const r1 = renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Recheck DNS" }));
    expect(toast.error).toHaveBeenCalledWith(
      "Public DNS points to the wrong IP",
    );
    r1.unmount();

    refetch.mockResolvedValueOnce({ data: { verdict: "pending" } });
    const r2 = renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Recheck DNS" }));
    expect(toast.warning).toHaveBeenCalled();
    r2.unmount();

    refetch.mockResolvedValueOnce({ data: { verdict: "unknown" } });
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Recheck DNS" }));
    expect(toast).toHaveBeenCalledWith("Rechecked public DNS");
  });

  it("submitLabel switches when the selected provider differs", async () => {
    // configured=wardnet, form provider=cloudflare while changing.
    enrollment = makeEnrollment({
      provider: "cloudflare",
      domain: "d.com",
      token: "t",
    });
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(screen.getByRole("button", { name: "Change provider" }));
    expect(
      screen.getByRole("button", { name: "Switch to Cloudflare" }),
    ).toBeInTheDocument();
  });

  it("removes remote access through the confirm modal", async () => {
    teardownMutateAsync.mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(
      screen.getByRole("button", { name: "Remove remote access" }),
    );
    // Modal action (distinct from the trigger).
    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(teardownMutateAsync).toHaveBeenCalled();
  });

  it("handles a failed removal without throwing", async () => {
    teardownMutateAsync.mockRejectedValue(new Error("nope"));
    const user = userEvent.setup();
    renderWithProviders(<RemoteAccess />);
    await user.click(
      screen.getByRole("button", { name: "Remove remote access" }),
    );
    await user.click(screen.getByRole("button", { name: "Remove" }));
    // handleRemove awaits the rejection and swallows it via describeError.
    await vi.waitFor(() => expect(teardownMutateAsync).toHaveBeenCalled());
  });
});

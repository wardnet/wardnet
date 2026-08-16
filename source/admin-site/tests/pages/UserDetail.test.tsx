/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  params,
  useUser,
  useUserCredentials,
  useEnrolments,
  useUpdateUserProfile,
  useUnlinkOauth,
  useIssueEnrolment,
  useRevokeEnrolment,
} = vi.hoisted(() => ({
  params: { current: { id: "u-ana" } as { id?: string } },
  useUser: vi.fn(),
  useUserCredentials: vi.fn(),
  useEnrolments: vi.fn(),
  useUpdateUserProfile: vi.fn(),
  useUnlinkOauth: vi.fn(),
  useIssueEnrolment: vi.fn(),
  useRevokeEnrolment: vi.fn(),
}));

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return { ...actual, useParams: () => params.current };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useUser,
    useUserCredentials,
    useEnrolments,
    useUpdateUserProfile,
    useUnlinkOauth,
    useIssueEnrolment,
    useRevokeEnrolment,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: any) => <h1>{title}</h1>,
}));

vi.mock("@/components/compound/ConfirmDialog", () => ({
  ConfirmDialog: ({ open, title, onConfirm }: any) =>
    open ? (
      <div data-testid="confirm">
        <span>{title}</span>
        <button onClick={onConfirm}>confirm</button>
      </div>
    ) : null,
}));

import UserDetail from "@/pages/UserDetail";
import { renderWithProviders } from "../test-utils";

function mutation(impl?: (vars: any, opts: any) => void) {
  return {
    mutate: vi.fn(impl ?? (() => {})),
    mutateAsync: vi.fn(),
    reset: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
  };
}

const ana = {
  id: "u-ana",
  display_name: "Ana",
  email: "ana@example.invalid",
  role: "admin",
  enabled: true,
  created_at: "",
  updated_at: "",
};

const passwordCred = {
  id: "c1",
  kind: "password",
  subject: "ana",
  label: null,
  created_at: "2026-08-01T00:00:00Z",
  last_used_at: null,
};
const githubCred = {
  id: "c2",
  kind: "github",
  subject: "12345",
  label: "ana-on-github",
  created_at: "2026-08-01T00:00:00Z",
  last_used_at: "2026-08-03T00:00:00Z",
};
const passkeyCred = { ...passwordCred, id: "c3", kind: "passkey" };

describe("UserDetail", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    params.current = { id: "u-ana" };
    useUser.mockReturnValue({ data: ana, isLoading: false });
    useUserCredentials.mockReturnValue({ data: [passwordCred] });
    useEnrolments.mockReturnValue({ data: [] });
    useUpdateUserProfile.mockReturnValue(mutation());
    useUnlinkOauth.mockReturnValue(mutation());
    useIssueEnrolment.mockReturnValue(mutation());
    useRevokeEnrolment.mockReturnValue(mutation());
  });

  it("saves a renamed profile, clearing a blanked email", async () => {
    const update = mutation();
    useUpdateUserProfile.mockReturnValue(update);
    renderWithProviders(<UserDetail />);

    await userEvent.clear(screen.getByLabelText("Email"));
    await userEvent.click(screen.getByRole("button", { name: "Save changes" }));

    expect(update.mutate).toHaveBeenCalledWith({
      id: "u-ana",
      body: { display_name: "Ana", email: null },
    });
  });

  it("offers no control for the local password", () => {
    // It is the floor — never removable, and only its owner may change it.
    renderWithProviders(<UserDetail />);
    expect(screen.getByText(/Only Ana can change it/)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Unlink" }),
    ).not.toBeInTheDocument();
  });

  it("offers unlink for a federated credential only", async () => {
    useUserCredentials.mockReturnValue({
      data: [passwordCred, githubCred],
    });
    const unlink = mutation();
    useUnlinkOauth.mockReturnValue(unlink);
    renderWithProviders(<UserDetail />);

    await userEvent.click(screen.getByRole("button", { name: "Unlink" }));
    await userEvent.click(screen.getByRole("button", { name: "confirm" }));

    expect(unlink.mutate).toHaveBeenCalledWith({
      id: "u-ana",
      provider: "github",
    });
  });

  it("offers no unlink for a passkey", () => {
    // The schema admits one (#1194) but it is not an OAuth provider, so the
    // unlink endpoint would reject it — the button would only ever 400.
    useUserCredentials.mockReturnValue({ data: [passkeyCred] });
    renderWithProviders(<UserDetail />);

    expect(
      screen.queryByRole("button", { name: "Unlink" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("Not manageable from here yet."),
    ).toBeInTheDocument();
  });

  it("says the account cannot sign in when it has no credentials", () => {
    useUserCredentials.mockReturnValue({ data: [] });
    renderWithProviders(<UserDetail />);
    expect(
      screen.getByText(/cannot sign in until an invitation/),
    ).toBeInTheDocument();
  });

  it("shows a freshly issued token once, with a redemption link", async () => {
    const issue = mutation((_id, opts) =>
      opts.onSuccess({
        token: "one-time-token",
        expires_at: "2026-08-04T00:00:00Z",
        user_id: "u-ana",
      }),
    );
    useIssueEnrolment.mockReturnValue(issue);
    renderWithProviders(<UserDetail />);

    await userEvent.click(screen.getByTestId("issue-enrolment"));

    expect(screen.getByText("one-time-token")).toBeInTheDocument();
    // A ready-made link beats asking somebody to retype 32 characters.
    expect(
      screen.getByText(/\/admin\/redeem\?token=one-time-token/),
    ).toBeInTheDocument();
    expect(screen.getByText(/only time it is shown/)).toBeInTheDocument();
  });

  it("dismisses the token once the admin has sent it", async () => {
    useIssueEnrolment.mockReturnValue(
      mutation((_id, opts) =>
        opts.onSuccess({
          token: "one-time-token",
          expires_at: "",
          user_id: "u-ana",
        }),
      ),
    );
    renderWithProviders(<UserDetail />);

    await userEvent.click(screen.getByTestId("issue-enrolment"));
    await userEvent.click(screen.getByRole("button", { name: /I've sent it/ }));

    expect(screen.queryByText("one-time-token")).not.toBeInTheDocument();
  });

  it("distinguishes open, spent and expired invitations", () => {
    useEnrolments.mockReturnValue({
      data: [
        {
          id: "e1",
          user_id: "u-ana",
          created_at: "2026-08-01T00:00:00Z",
          expires_at: "2999-01-01T00:00:00Z",
          used_at: null,
        },
        {
          id: "e2",
          user_id: "u-ana",
          created_at: "2026-07-01T00:00:00Z",
          expires_at: "2026-07-04T00:00:00Z",
          used_at: "2026-07-02T00:00:00Z",
        },
        {
          id: "e3",
          user_id: "u-ana",
          created_at: "2020-01-01T00:00:00Z",
          expires_at: "2020-01-04T00:00:00Z",
          used_at: null,
        },
      ],
    });
    renderWithProviders(<UserDetail />);

    expect(screen.getByText("Open")).toBeInTheDocument();
    expect(screen.getByText("Redeemed")).toBeInTheDocument();
    expect(screen.getByText("Expired")).toBeInTheDocument();
    // A spent invitation cannot be revoked; the other two can.
    expect(screen.getAllByRole("button", { name: "Revoke" })).toHaveLength(2);
  });

  it("revokes an outstanding invitation", async () => {
    const revoke = mutation();
    useRevokeEnrolment.mockReturnValue(revoke);
    useEnrolments.mockReturnValue({
      data: [
        {
          id: "e1",
          user_id: "u-ana",
          created_at: "",
          expires_at: "2999-01-01T00:00:00Z",
          used_at: null,
        },
      ],
    });
    renderWithProviders(<UserDetail />);

    await userEvent.click(screen.getByRole("button", { name: "Revoke" }));

    expect(revoke.mutate).toHaveBeenCalledWith({
      id: "u-ana",
      enrolmentId: "e1",
    });
  });

  it("surfaces a load failure and renders nothing else", () => {
    useUser.mockReturnValue({
      data: undefined,
      isLoading: false,
      error: new Error("boom"),
    });
    renderWithProviders(<UserDetail />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.queryByText("Invitations")).not.toBeInTheDocument();
  });

  it("waits rather than rendering a half-loaded user", () => {
    useUser.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<UserDetail />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });
});

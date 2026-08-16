/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useUsers,
  useCreateUser,
  useSetUserEnabled,
  useSetUserRole,
  useDeleteUser,
} = vi.hoisted(() => ({
  useUsers: vi.fn(),
  useCreateUser: vi.fn(),
  useSetUserEnabled: vi.fn(),
  useSetUserRole: vi.fn(),
  useDeleteUser: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useUsers,
    useCreateUser,
    useSetUserEnabled,
    useSetUserRole,
    useDeleteUser,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: any) => <h1>{title}</h1>,
}));

vi.mock("@/components/compound/ConfirmDialog", () => ({
  ConfirmDialog: ({ open, title, onConfirm, onOpenChange }: any) =>
    open ? (
      <div data-testid="confirm">
        <span>{title}</span>
        <button onClick={onConfirm}>confirm</button>
        <button onClick={() => onOpenChange(false)}>dismiss</button>
      </div>
    ) : null,
}));

import Users from "@/pages/Users";
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
const bruno = {
  id: "u-bruno",
  display_name: "Bruno",
  email: null,
  role: "member",
  enabled: true,
  created_at: "",
  updated_at: "",
};
const cleo = { ...bruno, id: "u-cleo", display_name: "Cleo", enabled: false };

describe("Users", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useUsers.mockReturnValue({ data: [ana, bruno], isLoading: false });
    useCreateUser.mockReturnValue(mutation());
    useSetUserEnabled.mockReturnValue(mutation());
    useSetUserRole.mockReturnValue(mutation());
    useDeleteUser.mockReturnValue(mutation());
  });

  it("lists the household with role and disabled state", () => {
    useUsers.mockReturnValue({ data: [ana, bruno, cleo], isLoading: false });
    renderWithProviders(<Users />);

    const rows = screen.getAllByTestId("user-row");
    expect(rows).toHaveLength(3);
    expect(within(rows[0]).getByText("Ana")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Bruno")).toBeInTheDocument();
    // "Admin" appears twice in an admin's row — the pill and the role select —
    // so this asserts presence rather than a single node.
    expect(within(rows[0]).getAllByText("Admin").length).toBeGreaterThan(0);
    // Only a *disabled* user is called out, and only in their own row.
    expect(within(rows[2]).getByText("Disabled")).toBeInTheDocument();
    expect(within(rows[1]).queryByText("Disabled")).not.toBeInTheDocument();
  });

  it("says so when the household is empty", () => {
    useUsers.mockReturnValue({ data: [], isLoading: false });
    renderWithProviders(<Users />);
    expect(screen.getByText("No users yet.")).toBeInTheDocument();
  });

  it("surfaces a load failure", () => {
    useUsers.mockReturnValue({
      data: undefined,
      isLoading: false,
      error: new Error("boom"),
    });
    renderWithProviders(<Users />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("creates a user with a null email when the field is left blank", async () => {
    const create = mutation();
    useCreateUser.mockReturnValue(create);
    renderWithProviders(<Users />);

    await userEvent.click(screen.getByRole("button", { name: "Add user" }));
    await userEvent.type(screen.getByTestId("user-name"), "Cleo");
    await userEvent.click(screen.getByTestId("user-submit"));

    // Empty means "no email", not an empty address — the column is uniquely
    // indexed and a second empty string would collide.
    expect(create.mutate).toHaveBeenCalledWith(
      { display_name: "Cleo", email: null, role: "member" },
      expect.anything(),
    );
  });

  it("refuses to submit a nameless user", async () => {
    const create = mutation();
    useCreateUser.mockReturnValue(create);
    renderWithProviders(<Users />);

    await userEvent.click(screen.getByRole("button", { name: "Add user" }));
    await userEvent.click(screen.getByTestId("user-submit"));

    expect(create.mutate).not.toHaveBeenCalled();
  });

  it("locks the last enabled admin against disable, demote and delete", () => {
    // The daemon refuses all three; disabling the controls says so before the
    // click rather than by error toast afterwards.
    useUsers.mockReturnValue({ data: [ana, bruno], isLoading: false });
    renderWithProviders(<Users />);

    const adminRow = screen.getAllByTestId("user-row")[0];
    expect(
      within(adminRow).getByRole("button", { name: "Disable" }),
    ).toBeDisabled();
    expect(
      within(adminRow).getByRole("button", { name: "Delete" }),
    ).toBeDisabled();
    expect(
      screen.getByText(/last enabled admin cannot be disabled/i),
    ).toBeInTheDocument();
  });

  it("frees those controls once a second admin exists", () => {
    useUsers.mockReturnValue({
      data: [ana, { ...bruno, role: "admin" }],
      isLoading: false,
    });
    renderWithProviders(<Users />);

    const adminRow = screen.getAllByTestId("user-row")[0];
    expect(
      within(adminRow).getByRole("button", { name: "Disable" }),
    ).toBeEnabled();
    expect(
      within(adminRow).getByRole("button", { name: "Delete" }),
    ).toBeEnabled();
  });

  it("toggles a member's enabled state", async () => {
    const setEnabled = mutation();
    useSetUserEnabled.mockReturnValue(setEnabled);
    renderWithProviders(<Users />);

    const memberRow = screen.getAllByTestId("user-row")[1];
    await userEvent.click(
      within(memberRow).getByRole("button", { name: "Disable" }),
    );

    expect(setEnabled.mutate).toHaveBeenCalledWith({
      id: "u-bruno",
      enabled: false,
    });
  });

  it("deletes only after the confirmation is accepted", async () => {
    const del = mutation();
    useDeleteUser.mockReturnValue(del);
    renderWithProviders(<Users />);

    const memberRow = screen.getAllByTestId("user-row")[1];
    await userEvent.click(
      within(memberRow).getByRole("button", { name: "Delete" }),
    );
    expect(del.mutate).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "confirm" }));
    expect(del.mutate).toHaveBeenCalledWith("u-bruno");
  });

  it("closes and clears the form once the user is created", async () => {
    // The reset matters: leaving the previous name in the box invites an
    // admin to submit it twice.
    useCreateUser.mockReturnValue(
      mutation((_vars, opts) => opts.onSuccess?.()),
    );
    renderWithProviders(<Users />);

    await userEvent.click(screen.getByRole("button", { name: "Add user" }));
    await userEvent.type(screen.getByTestId("user-name"), "Cleo");
    await userEvent.click(screen.getByTestId("user-submit"));

    expect(screen.queryByTestId("user-name")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add user" }));
    expect(screen.getByTestId("user-name")).toHaveValue("");
  });

  it("carries a typed email through to the create call", async () => {
    const create = mutation();
    useCreateUser.mockReturnValue(create);
    renderWithProviders(<Users />);

    await userEvent.click(screen.getByRole("button", { name: "Add user" }));
    await userEvent.type(screen.getByTestId("user-name"), "Cleo");
    await userEvent.type(
      screen.getByLabelText("Email"),
      "cleo@example.invalid",
    );
    await userEvent.click(screen.getByTestId("user-submit"));

    expect(create.mutate).toHaveBeenCalledWith(
      expect.objectContaining({ email: "cleo@example.invalid" }),
      expect.anything(),
    );
  });

  it("abandons the create form on cancel", async () => {
    renderWithProviders(<Users />);

    await userEvent.click(screen.getByRole("button", { name: "Add user" }));
    expect(screen.getByTestId("user-name")).toBeInTheDocument();

    // Two controls read "Cancel" while the form is open — the header toggle
    // and the form's own secondary action. This exercises the latter.
    const cancels = screen.getAllByRole("button", { name: "Cancel" });
    await userEvent.click(cancels[cancels.length - 1]);
    expect(screen.queryByTestId("user-name")).not.toBeInTheDocument();
  });

  it("keeps the user when the delete dialog is dismissed", async () => {
    const del = mutation();
    useDeleteUser.mockReturnValue(del);
    renderWithProviders(<Users />);

    const memberRow = screen.getAllByTestId("user-row")[1];
    await userEvent.click(
      within(memberRow).getByRole("button", { name: "Delete" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "dismiss" }));

    expect(del.mutate).not.toHaveBeenCalled();
    expect(screen.queryByTestId("confirm")).not.toBeInTheDocument();
  });

  it("names the person in the delete confirmation", async () => {
    renderWithProviders(<Users />);
    const memberRow = screen.getAllByTestId("user-row")[1];
    await userEvent.click(
      within(memberRow).getByRole("button", { name: "Delete" }),
    );
    expect(screen.getByTestId("confirm")).toHaveTextContent("Delete Bruno?");
  });
});

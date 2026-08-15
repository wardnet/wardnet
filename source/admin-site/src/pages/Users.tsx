import { useState } from "react";
import { Link } from "react-router";
import { PageHeader } from "@/components/compound/PageHeader";
import { ConfirmDialog } from "@/components/compound/ConfirmDialog";
import {
  ApiErrorAlert,
  Button,
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
  Field,
  Form,
  FormActions,
  Input,
  Pill,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Text,
  Validator,
  useCreateUser,
  useDeleteUser,
  useSetUserEnabled,
  useSetUserRole,
  useUsers,
} from "@wardnet/web";
import type { User, UserRole } from "@wardnet/js";

/**
 * The household directory (ADR-0031, issue #1147).
 *
 * A new user is created with **no credential** and cannot sign in until an
 * invitation is redeemed, so each row links to the detail page where
 * invitations live. Nothing here can set somebody else's password, by design:
 * an admin who could would then know it.
 */
export default function Users() {
  const { data: users, isLoading, error } = useUsers();
  const createUser = useCreateUser();
  const setEnabled = useSetUserEnabled();
  const setRole = useSetUserRole();
  const deleteUser = useDeleteUser();

  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [role, setRoleValue] = useState<UserRole>("member");
  const [pendingDelete, setPendingDelete] = useState<User | null>(null);

  // The daemon refuses to remove, disable or demote the last enabled admin — a
  // box with no admin cannot be administered, and there is no way in from
  // outside the network. Disabling the controls says so before the click
  // rather than after it.
  const enabledAdmins = (users ?? []).filter(
    (u) => u.role === "admin" && u.enabled,
  ).length;
  const isLastEnabledAdmin = (u: User) =>
    u.role === "admin" && u.enabled && enabledAdmins <= 1;

  function handleCreate(values: { display_name: string }) {
    createUser.mutate(
      {
        display_name: values.display_name.trim(),
        // Empty means "no email", not an empty address: the column is uniquely
        // indexed, so a run of empty strings would collide on the second user.
        email: email.trim() === "" ? null : email.trim(),
        role,
      },
      {
        onSuccess: () => {
          setAdding(false);
          setName("");
          setEmail("");
          setRoleValue("member");
        },
      },
    );
  }

  return (
    <>
      <PageHeader
        title="Users"
        description="Everyone who can sign in to this Wardnet. Accounts start empty — you send an invitation, and the person sets their own password, which you never see."
      />

      {error && <ApiErrorAlert error={error} />}

      <Card>
        <CardHeader>
          <CardTitle>Household</CardTitle>
          <CardAction>
            <Button variant="secondary" onClick={() => setAdding((v) => !v)}>
              {adding ? "Cancel" : "Add user"}
            </Button>
          </CardAction>
        </CardHeader>

        {adding && (
          <Form values={{ display_name: name }} onSubmit={handleCreate}>
            <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 md:grid-cols-2">
              <div>
                <Field label="Name" htmlFor="user-name" name="display_name">
                  <Input
                    id="user-name"
                    data-testid="user-name"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="Bruno"
                  />
                </Field>
                <Validator
                  name="display_name"
                  rule="required"
                  message="A name is required."
                />
              </div>

              <Field
                label="Email"
                htmlFor="user-email"
                help="Optional. Only needed to match a Google or GitHub account later."
              >
                <Input
                  id="user-email"
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="bruno@example.com"
                />
              </Field>

              <Field
                label="Role"
                help="An admin can change anything on this Wardnet. A member uses the network and manages only their own profile."
                className="md:col-span-2"
              >
                <Select
                  value={role}
                  onValueChange={(v) => setRoleValue(v as UserRole)}
                >
                  <SelectTrigger data-testid="user-role">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="member">Member</SelectItem>
                    <SelectItem value="admin">Admin</SelectItem>
                  </SelectContent>
                </Select>
              </Field>
            </CardContent>

            <FormActions
              secondaryLabel="Cancel"
              secondaryProps={{
                type: "button",
                onClick: () => setAdding(false),
                disabled: createUser.isPending,
              }}
              primaryLabel="Add user"
              primaryProps={{
                type: "submit",
                disabled: createUser.isPending,
                "data-testid": "user-submit",
              }}
            />
          </Form>
        )}

        <CardContent className="flex flex-col gap-1">
          {isLoading && <Text size="sm">Loading…</Text>}

          {users?.length === 0 && (
            <Text size="sm" className="text-ink-3">
              No users yet.
            </Text>
          )}

          {users?.map((user) => (
            <div
              key={user.id}
              className="flex flex-wrap items-center gap-3 border-b border-line py-3 last:border-0"
              data-testid="user-row"
            >
              <div className="flex min-w-0 flex-1 flex-col">
                <Link to={`/users/${user.id}`} className="truncate">
                  <Text as="span" size="sm" weight="medium">
                    {user.display_name}
                  </Text>
                </Link>
                <Text size="xs" className="truncate text-ink-3">
                  {user.email ?? "No email"}
                </Text>
              </div>

              {user.role === "admin" && <Pill variant="info">Admin</Pill>}
              {!user.enabled && <Pill variant="warn">Disabled</Pill>}

              <Select
                value={user.role}
                disabled={isLastEnabledAdmin(user) || setRole.isPending}
                onValueChange={(next) =>
                  setRole.mutate({ id: user.id, role: next as UserRole })
                }
              >
                <SelectTrigger className="w-32">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="member">Member</SelectItem>
                  <SelectItem value="admin">Admin</SelectItem>
                </SelectContent>
              </Select>

              <Button
                variant="secondary"
                disabled={isLastEnabledAdmin(user) || setEnabled.isPending}
                onClick={() =>
                  setEnabled.mutate({ id: user.id, enabled: !user.enabled })
                }
              >
                {user.enabled ? "Disable" : "Enable"}
              </Button>

              <Button
                variant="destructive"
                disabled={isLastEnabledAdmin(user)}
                onClick={() => setPendingDelete(user)}
              >
                Delete
              </Button>
            </div>
          ))}

          {enabledAdmins <= 1 && (users?.length ?? 0) > 0 && (
            <Text size="xs" className="pt-2 text-ink-3">
              The last enabled admin cannot be disabled, demoted, or deleted —
              nobody could administer this Wardnet afterwards, and there is no
              way in from outside your network.
            </Text>
          )}
        </CardContent>
      </Card>

      <ConfirmDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null);
        }}
        title={`Delete ${pendingDelete?.display_name ?? ""}?`}
        description="Their sign-in credentials and invitations are removed. Devices they own are kept and simply become unassigned — deleting a person never deletes hardware."
        confirmLabel="Delete user"
        onConfirm={() => {
          if (pendingDelete) deleteUser.mutate(pendingDelete.id);
          setPendingDelete(null);
        }}
      />
    </>
  );
}

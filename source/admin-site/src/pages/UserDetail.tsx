import { useState } from "react";
import { useParams } from "react-router";
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
  CopyButton,
  Field,
  Form,
  FormActions,
  Input,
  Pill,
  Text,
  Validator,
  useEnrolments,
  useIssueEnrolment,
  useRevokeEnrolment,
  useUnlinkOauth,
  useUpdateUserProfile,
  useUser,
  useUserCredentials,
} from "@wardnet/web";
import type {
  CredentialKind,
  EnrolmentInvite,
  OauthProvider,
} from "@wardnet/js";

const CREDENTIAL_LABEL: Record<CredentialKind, string> = {
  password: "Wardnet password",
  google: "Google",
  github: "GitHub",
  passkey: "Passkey",
};

function formatDate(iso: string | null): string {
  return iso ? new Date(iso).toLocaleString() : "Never";
}

/**
 * An absolute link to the redemption page, pre-filled with the token.
 *
 * Absolute because it is about to leave this browser — pasted into a message
 * — so a relative path would be useless. Built from `window.location.origin`
 * rather than the box's canonical hostname: whatever address the admin is
 * reaching the UI on is one that demonstrably works from inside the house,
 * and a member redeeming an invitation is on that same network.
 */
function redeemLink(token: string): string {
  return `${window.location.origin}/admin/redeem?token=${encodeURIComponent(token)}`;
}

/**
 * One household user: profile, credentials, and invitations.
 *
 * Invitations live here rather than on a screen of their own because the API
 * has no list-all endpoint — an invitation exists only relative to the person
 * it enrols, so a global list would be a view with no source of truth.
 */
/**
 * Remount the page whenever the `:id` changes.
 *
 * React Router reuses a route element across param changes, so without this
 * the state below survives navigation from one user to another — including
 * `issued`, the enrolment token shown exactly once. It would then render under
 * the **wrong** person's name, with a copy button beside it, and an admin
 * copying it would hand out a credential for an account they were not looking
 * at. Keying is what makes that unrepresentable rather than remembered.
 */
export default function UserDetail() {
  const { id } = useParams<{ id: string }>();
  return <UserDetailView key={id} id={id} />;
}

function UserDetailView({ id }: { id: string | undefined }) {
  const { data: user, isLoading, error } = useUser(id);
  const { data: credentials } = useUserCredentials(id);
  const { data: enrolments } = useEnrolments(id);

  const updateProfile = useUpdateUserProfile();
  const unlinkOauth = useUnlinkOauth();
  const issueEnrolment = useIssueEnrolment();
  const revokeEnrolment = useRevokeEnrolment();

  const [name, setName] = useState<string | null>(null);
  const [email, setEmail] = useState<string | null>(null);
  // The freshly issued token, held here because the daemon returns it
  // **exactly once** — only its hash is stored. Refetching the list will never
  // bring it back, so it must stay on screen until the admin has copied it.
  const [issued, setIssued] = useState<EnrolmentInvite | null>(null);
  const [pendingUnlink, setPendingUnlink] = useState<OauthProvider | null>(
    null,
  );

  if (error) return <ApiErrorAlert error={error} />;
  if (isLoading || !user || !id) return <Text size="sm">Loading…</Text>;

  // `null` means "not edited yet", so the fields track the server value until
  // the admin actually types something.
  const nameValue = name ?? user.display_name;
  const emailValue = email ?? user.email ?? "";

  const hasPassword = credentials?.some((c) => c.kind === "password") ?? false;
  const openInvitations = (enrolments ?? []).filter((e) => e.used_at === null);

  return (
    <>
      <PageHeader
        title={user.display_name}
        description={user.email ?? "No email address"}
      />

      <Card>
        <CardHeader>
          <CardTitle>Profile</CardTitle>
          <CardAction>
            {user.role === "admin" && <Pill variant="info">Admin</Pill>}
            {!user.enabled && <Pill variant="warn">Disabled</Pill>}
          </CardAction>
        </CardHeader>

        <Form
          values={{ display_name: nameValue }}
          onSubmit={(values: { display_name: string }) =>
            updateProfile.mutate({
              id,
              body: {
                display_name: values.display_name.trim(),
                email: emailValue.trim() === "" ? null : emailValue.trim(),
              },
            })
          }
        >
          <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 md:grid-cols-2">
            <div>
              <Field label="Name" htmlFor="profile-name" name="display_name">
                <Input
                  id="profile-name"
                  value={nameValue}
                  onChange={(e) => setName(e.target.value)}
                />
              </Field>
              <Validator
                name="display_name"
                rule="required"
                message="A name is required."
              />
            </div>

            <Field label="Email" htmlFor="profile-email" help="Optional.">
              <Input
                id="profile-email"
                type="email"
                value={emailValue}
                onChange={(e) => setEmail(e.target.value)}
              />
            </Field>
          </CardContent>

          <FormActions
            primaryLabel="Save changes"
            primaryProps={{
              type: "submit",
              disabled: updateProfile.isPending,
            }}
          />
        </Form>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Sign-in methods</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-1">
          {credentials?.length === 0 && (
            <Text size="sm" className="text-ink-3">
              None yet — this account cannot sign in until an invitation below
              is redeemed.
            </Text>
          )}

          {credentials?.map((cred) => {
            // Narrow once, here, rather than casting at the call site: a
            // passkey is a credential kind the schema admits (#1194) but is
            // not an OAuth provider, so `cred.kind as OauthProvider` would be
            // a lie that renders an Unlink button the API can only 400.
            const unlinkable: OauthProvider | null =
              cred.kind === "google" || cred.kind === "github"
                ? cred.kind
                : null;
            return (
              <div
                key={cred.id}
                className="flex flex-wrap items-center gap-3 border-b border-line py-2 last:border-0"
              >
                <div className="flex min-w-0 flex-1 flex-col">
                  <Text size="sm" weight="medium">
                    {CREDENTIAL_LABEL[cred.kind]}
                  </Text>
                  <Text size="xs" className="truncate text-ink-3">
                    {cred.label ?? cred.subject} · last used{" "}
                    {formatDate(cred.last_used_at)}
                  </Text>
                </div>

                {unlinkable ? (
                  <Button
                    variant="secondary"
                    onClick={() => setPendingUnlink(unlinkable)}
                  >
                    Unlink
                  </Button>
                ) : cred.kind === "password" ? (
                  // Deliberately no control here. The password is the floor — an
                  // account whose only credential needed a reachable provider
                  // would be locked out during an internet outage — and only its
                  // owner can change it, never an admin.
                  <Text size="xs" className="text-ink-3">
                    Always available. Only {user.display_name} can change it.
                  </Text>
                ) : (
                  // A passkey is a credential kind the schema admits but no code
                  // path produces yet (#1194). It is not an OAuth provider, so
                  // there is no unlink endpoint that would accept it — offering
                  // the button would only ever produce a 400.
                  <Text size="xs" className="text-ink-3">
                    Not manageable from here yet.
                  </Text>
                )}
              </div>
            );
          })}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Invitations</CardTitle>
          <CardAction>
            <Button
              disabled={issueEnrolment.isPending}
              onClick={() =>
                issueEnrolment.mutate(id, { onSuccess: setIssued })
              }
              data-testid="issue-enrolment"
            >
              {issueEnrolment.isPending ? "Creating…" : "New invitation"}
            </Button>
          </CardAction>
        </CardHeader>

        <CardContent className="flex flex-col gap-3">
          {issued && (
            <div className="flex flex-col gap-2 rounded-md border border-line p-3">
              <Text size="sm" weight="medium">
                Send this to {user.display_name} now
              </Text>
              <div className="flex items-center gap-2">
                <Text
                  as="code"
                  size="sm"
                  className="min-w-0 flex-1 truncate font-mono"
                >
                  {issued.token}
                </Text>
                <CopyButton value={issued.token} />
              </div>

              {/*
                A ready-made link as well as the bare code. The redemption page
                pre-fills from `?token=`, which beats asking somebody to
                retype 32 bytes of base64 — while the code itself stays
                available for handing over by message or out loud.
              */}
              <div className="flex items-center gap-2">
                <Text
                  as="code"
                  size="sm"
                  className="min-w-0 flex-1 truncate font-mono"
                >
                  {redeemLink(issued.token)}
                </Text>
                <CopyButton value={redeemLink(issued.token)} />
              </div>

              <Text size="xs" className="text-ink-3">
                This is the only time it is shown — Wardnet keeps only a hash of
                it. If you lose it, create another. Valid until{" "}
                {formatDate(issued.expires_at)}.
              </Text>
              <div>
                <Button variant="secondary" onClick={() => setIssued(null)}>
                  I&apos;ve sent it
                </Button>
              </div>
            </div>
          )}

          {hasPassword && openInvitations.length === 0 && !issued && (
            <Text size="sm" className="text-ink-3">
              {user.display_name} already has a password. A new invitation lets
              them set another one — useful if they are locked out.
            </Text>
          )}

          {enrolments?.length === 0 && !issued && (
            <Text size="sm" className="text-ink-3">
              No invitations yet.
            </Text>
          )}

          {enrolments?.map((enrolment) => {
            const spent = enrolment.used_at !== null;
            const expired =
              !spent && new Date(enrolment.expires_at) < new Date();
            return (
              <div
                key={enrolment.id}
                className="flex flex-wrap items-center gap-3 border-b border-line py-2 last:border-0"
              >
                <div className="flex min-w-0 flex-1 flex-col">
                  <Text size="sm">
                    Created {formatDate(enrolment.created_at)}
                  </Text>
                  <Text size="xs" className="text-ink-3">
                    {spent
                      ? `Redeemed ${formatDate(enrolment.used_at)}`
                      : `Expires ${formatDate(enrolment.expires_at)}`}
                  </Text>
                </div>

                {spent && <Pill variant="ghost">Redeemed</Pill>}
                {expired && <Pill variant="warn">Expired</Pill>}
                {!spent && !expired && <Pill variant="ok">Open</Pill>}

                {!spent && (
                  <Button
                    variant="secondary"
                    onClick={() =>
                      revokeEnrolment.mutate({ id, enrolmentId: enrolment.id })
                    }
                  >
                    Revoke
                  </Button>
                )}
              </div>
            );
          })}
        </CardContent>
      </Card>

      <ConfirmDialog
        open={pendingUnlink !== null}
        onOpenChange={(open) => {
          if (!open) setPendingUnlink(null);
        }}
        title="Unlink this account?"
        description={`${user.display_name} will no longer be able to sign in with it. Their Wardnet password still works.`}
        confirmLabel="Unlink"
        onConfirm={() => {
          if (pendingUnlink)
            unlinkOauth.mutate({ id, provider: pendingUnlink });
          setPendingUnlink(null);
        }}
      />
    </>
  );
}

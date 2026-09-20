import { useState } from "react";
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
  Toggle,
  Validator,
  useClearOauthProvider,
  useConfigureOauthProvider,
  useOauthProviders,
} from "@wardnet/web";
import type { OauthProviderConfig, OauthProvider } from "@wardnet/js";

const PROVIDER_LABEL: Record<string, string> = {
  google: "Google",
  github: "GitHub",
};

function ProviderCard({ status }: { status: OauthProviderConfig }) {
  const configure = useConfigureOauthProvider();
  const clear = useClearOauthProvider();

  const [editing, setEditing] = useState(false);
  const [clientId, setClientId] = useState<string | null>(null);
  const [clientSecret, setClientSecret] = useState("");
  const [confirmClear, setConfirmClear] = useState(false);

  const provider = status.provider as OauthProvider;
  const label = PROVIDER_LABEL[status.provider] ?? status.provider;
  const clientIdValue = clientId ?? status.client_id ?? "";

  return (
    <Card>
      <CardHeader>
        <CardTitle>{label}</CardTitle>
        <CardAction>
          {/*
            Exactly one pill. `enabled` here is the *stored* flag and
            `configured` is "a secret is present", so the two are independent —
            a provider switched on whose secret was later cleared would
            otherwise render "On" and "Not set up" side by side. Setup is the
            more urgent truth, so it wins.
          */}
          {!status.configured ? (
            <Pill variant="warn">Not set up</Pill>
          ) : status.enabled ? (
            <Pill variant="ok">On</Pill>
          ) : (
            <Pill variant="ghost">Off</Pill>
          )}
        </CardAction>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {status.redirect_uri ? (
          <div className="flex flex-col gap-1">
            <Text size="sm" weight="medium">
              Redirect URI
            </Text>
            <div className="flex items-center gap-2">
              <Text
                as="code"
                size="sm"
                className="min-w-0 flex-1 truncate font-mono"
              >
                {status.redirect_uri}
              </Text>
              <CopyButton value={status.redirect_uri} />
            </div>
            <Text size="xs" className="text-ink-3">
              Paste this into your {label} app exactly as shown. Sign-in fails
              at {label} if it does not match character for character.
            </Text>
          </div>
        ) : (
          // Without a public hostname the provider has nowhere to send people
          // back to, so no amount of filling in this form will make it work.
          // Saying that beats rendering a button that fails at the provider.
          <Text size="sm" className="text-ink-3">
            This Wardnet has no public address yet, so {label} has nowhere to
            send people back to. Set up Remote access first.
          </Text>
        )}

        {!editing && (
          <div className="flex flex-wrap items-center gap-3">
            <Toggle
              checked={status.enabled}
              // Enabling needs a client id, a secret and a public address. A
              // toggle that flipped without them would report "on" for a
              // provider that cannot complete a single ceremony.
              disabled={
                !status.configured ||
                !status.redirect_uri ||
                configure.isPending
              }
              aria-label={`Offer ${label} sign-in`}
              onCheckedChange={(enabled) =>
                configure.mutate({
                  provider,
                  body: {
                    // Resubmit the stored client id unchanged. The API returns
                    // it precisely so a toggle need not know it — sending ""
                    // would erase the configuration being switched on.
                    client_id: status.client_id ?? "",
                    // `null` keeps the stored secret, which this form cannot
                    // read back and must not clear.
                    client_secret: null,
                    enabled,
                  },
                })
              }
            />
            <Text size="sm">Offer {label} sign-in</Text>

            <Button variant="secondary" onClick={() => setEditing(true)}>
              {status.configured ? "Update credentials" : "Set up"}
            </Button>

            {status.configured && (
              <Button
                variant="destructive"
                onClick={() => setConfirmClear(true)}
              >
                Remove
              </Button>
            )}
          </div>
        )}
      </CardContent>

      {editing && (
        <Form
          values={{ client_id: clientIdValue }}
          onSubmit={(values: { client_id: string }) =>
            configure.mutate(
              {
                provider,
                body: {
                  client_id: values.client_id.trim(),
                  // Blank means "keep the stored secret". The form cannot read
                  // it, so a blank field must not be taken as "erase it".
                  client_secret:
                    clientSecret.trim() === "" ? null : clientSecret.trim(),
                  enabled: status.enabled,
                },
              },
              {
                onSuccess: () => {
                  setEditing(false);
                  setClientSecret("");
                  setClientId(null);
                },
              },
            )
          }
        >
          <CardContent className="grid grid-cols-1 gap-x-6 gap-y-5 md:grid-cols-2">
            <div>
              <Field
                label="Client ID"
                htmlFor={`${provider}-client-id`}
                name="client_id"
              >
                <Input
                  id={`${provider}-client-id`}
                  autoComplete="off"
                  value={clientIdValue}
                  onChange={(e) => setClientId(e.target.value)}
                />
              </Field>
              <Validator
                name="client_id"
                rule="required"
                message="A client ID is required."
              />
            </div>

            <Field
              label="Client secret"
              htmlFor={`${provider}-client-secret`}
              help={
                status.configured
                  ? "Leave blank to keep the secret you already saved."
                  : "Copied from your app's credentials page. Wardnet never shows it again."
              }
            >
              <Input
                id={`${provider}-client-secret`}
                type="password"
                autoComplete="off"
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
              />
            </Field>
          </CardContent>

          <FormActions
            secondaryLabel="Cancel"
            secondaryProps={{
              type: "button",
              onClick: () => {
                setEditing(false);
                setClientId(null);
                setClientSecret("");
              },
              disabled: configure.isPending,
            }}
            primaryLabel="Save"
            primaryProps={{ type: "submit", disabled: configure.isPending }}
          />
        </Form>
      )}

      <ConfirmDialog
        open={confirmClear}
        onOpenChange={setConfirmClear}
        title={`Remove ${label} sign-in?`}
        description={`The client ID and secret are deleted. Anyone who linked their ${label} account keeps their Wardnet password and can still sign in with it.`}
        confirmLabel="Remove"
        onConfirm={() => {
          clear.mutate(provider);
          setConfirmClear(false);
        }}
      />
    </Card>
  );
}

/**
 * Federated sign-in setup (ADR-0031 §2).
 *
 * Each household registers its own OAuth app, so this screen's real job is
 * handing over the exact redirect URI to paste. Secrets are write-only
 * throughout: the daemon reports only whether one is present.
 */
export default function SignInMethods() {
  // The **admin** provider endpoint, not `useAuthMethods`. That one is the
  // unauthenticated sign-in contract: it carries no client ID, and its
  // `enabled` is the effective availability rather than the stored flag — so
  // a form built on it would write a computed value back and silently switch
  // a provider off whenever its hostname or secret was momentarily absent.
  const { data: providers, isLoading, error } = useOauthProviders();

  return (
    <>
      <PageHeader
        title="Sign-in methods"
        description="Let people sign in with a Google or GitHub account instead of a password. Entirely optional — a Wardnet password always works, including when the internet is down."
      />

      {error && <ApiErrorAlert error={error} />}
      {isLoading && <Text size="sm">Loading…</Text>}

      {providers && (
        <>
          <Card>
            <CardHeader>
              <CardTitle>Wardnet password</CardTitle>
              <CardAction>
                <Pill variant="ok">Always on</Pill>
              </CardAction>
            </CardHeader>
            <CardContent>
              <Text size="sm" className="text-ink-3">
                Cannot be turned off. It needs no internet connection, no
                certificate and no outside company — which is what makes it the
                one way in that always works.
              </Text>
            </CardContent>
          </Card>

          {providers.map((status) => (
            <ProviderCard key={status.provider} status={status} />
          ))}
        </>
      )}
    </>
  );
}

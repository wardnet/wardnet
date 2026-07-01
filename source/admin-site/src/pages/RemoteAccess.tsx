import { useState } from "react";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  AlertModal,
  AlertModalAction,
  AlertModalCancel,
  AlertModalContent,
  AlertModalDescription,
  AlertModalFooter,
  AlertModalHeader,
  AlertModalTitle,
} from "@wardnet/web";
import { toast } from "@wardnet/ui";
import { WardnetApiError } from "@wardnet/js";
import {
  useDdnsStatus,
  useDeleteDdns,
  useResolutionCheck,
  useTlsStatus,
} from "@wardnet/web";
import { PageHeader } from "@/components/compound/PageHeader";
import { RemoteAccessStatus } from "@/components/features/RemoteAccessStatus";
import {
  CloudflareFields,
  ProviderOption,
  WardnetFields,
} from "@/components/features/wardnet-enrollment";
import { useWardnetEnrollment, type Provider } from "@/lib/wardnet-enrollment";
import { suggestName } from "@/lib/suggestName";

/**
 * Remote access settings (admin only) — configure, switch, and monitor the
 * daemon-owned HTTPS provisioning the setup wizard introduces. Unlike the
 * wizard's one-shot step this page is the steady-state surface: it leads with
 * live status (phase, certificate, resolution check) and keeps the provider
 * form behind a "Change provider" action. The wardnet path runs the same
 * email → code → slug enrollment as the wizard (shared via
 * {@link useWardnetEnrollment}); enabling and switching reuse the same
 * `POST /api/ddns/{enrollment-code,enroll,register,cloudflare}` endpoints
 * (the daemon performs the new-first/teardown-old switch and kicks off
 * background issuance), so this page only drives them and polls
 * `GET /api/tls/status`.
 */
export default function RemoteAccess() {
  const { data: ddns } = useDdnsStatus();
  const { data: tls } = useTlsStatus();
  const configured = !!ddns?.provider;
  const resolution = useResolutionCheck(configured);

  const teardown = useDeleteDdns();

  // The provider form is shown when enabling (unconfigured) or after the
  // operator explicitly chooses to change an existing configuration.
  const [changing, setChanging] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [removeOpen, setRemoveOpen] = useState(false);

  function describeError(err: unknown): string {
    if (err instanceof WardnetApiError) return err.body.error;
    return "Couldn't reach the daemon. Please try again.";
  }

  const enrollment = useWardnetEnrollment({
    onProvisioned: () => setChanging(false),
    onError: (err) => setFormError(describeError(err)),
    clearError: () => setFormError(null),
  });
  const {
    provider,
    setProvider,
    wardnetStep,
    setWardnetStep,
    email,
    setEmail,
    code,
    setCode,
    slug,
    setSlug,
    token,
    setToken,
    domain,
    setDomain,
    availability,
    busy,
    slugDisabled,
    pending,
    reset,
  } = enrollment;

  function selectProvider(next: Provider) {
    setFormError(null);
    setProvider(next);
    if (next === "wardnet") setWardnetStep("email");
  }

  function openChange() {
    setFormError(null);
    setProvider(ddns?.provider === "cloudflare" ? "cloudflare" : "wardnet");
    reset();
    setChanging(true);
  }

  async function handleRemove() {
    setFormError(null);
    try {
      await teardown.mutateAsync();
      setRemoveOpen(false);
    } catch (err) {
      setFormError(describeError(err));
    }
  }

  /** Manual re-check: a toast confirms it ran (the status row also updates a
   *  "Checked …" timestamp, so an unchanged verdict is still visibly fresh). */
  async function handleRecheck() {
    const res = await resolution.refetch();
    switch (res.data?.verdict) {
      case "match":
        toast.success("Public DNS resolves correctly");
        break;
      case "mismatch":
        toast.error("Public DNS points to the wrong IP");
        break;
      case "pending":
        toast.warning("Public DNS isn't visible yet — still propagating");
        break;
      default:
        toast("Rechecked public DNS");
    }
  }

  /** Action verb for the slug/cloudflare submit button. */
  function submitLabel(): string {
    if (!configured) return "Enable remote access";
    if (provider !== ddns?.provider) {
      return `Switch to ${provider === "wardnet" ? "Wardnet" : "Cloudflare"}`;
    }
    return provider === "wardnet"
      ? "Register a new hostname"
      : "Update Cloudflare settings";
  }

  // Provider picker + inputs (no buttons — actions live in the card footer).
  const formFields = (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <ProviderOption
          label="Wardnet"
          description="Zero-config. We assign a hostname under wardnet.services and handle DNS — enroll with your wardnet account."
          selected={provider === "wardnet"}
          onSelect={() => selectProvider("wardnet")}
        />
        <ProviderOption
          label="Your own domain (Cloudflare)"
          description="Use a domain you control via a Cloudflare API token."
          selected={provider === "cloudflare"}
          onSelect={() => selectProvider("cloudflare")}
        />
      </div>

      {provider === "wardnet" ? (
        <WardnetFields
          step={wardnetStep}
          email={email}
          onEmailChange={setEmail}
          code={code}
          onCodeChange={setCode}
          slug={slug}
          onSlugChange={setSlug}
          availability={availability}
          onSuggest={() => setSlug(suggestName())}
          onChangeEmail={() => setWardnetStep("email")}
        />
      ) : (
        <CloudflareFields
          domain={domain}
          onDomainChange={setDomain}
          token={token}
          onTokenChange={setToken}
        />
      )}

      {formError && (
        <Text as="p" size="sm" className="text-danger">
          {formError}
        </Text>
      )}
    </div>
  );

  // Footer actions for the provider form (Cancel only when changing an existing
  // config). Each action gates only on its own mutation.
  const formActions = (
    <>
      {changing && (
        <Button
          variant="ghost"
          onClick={() => setChanging(false)}
          disabled={busy}
        >
          Cancel
        </Button>
      )}
      {provider === "wardnet" ? (
        wardnetStep === "email" ? (
          <Button
            onClick={enrollment.sendCode}
            disabled={pending.sendCode || !email.includes("@")}
          >
            {pending.sendCode ? "Sending…" : "Send code"}
          </Button>
        ) : wardnetStep === "code" ? (
          <Button
            onClick={enrollment.verifyCode}
            disabled={pending.verify || code.trim().length === 0}
          >
            {pending.verify ? "Verifying…" : "Verify code"}
          </Button>
        ) : (
          <Button onClick={enrollment.registerWardnet} disabled={slugDisabled}>
            {pending.register ? "Working…" : submitLabel()}
          </Button>
        )
      ) : (
        <Button
          onClick={enrollment.enableCloudflare}
          disabled={
            pending.configureCf || domain.length === 0 || token.length === 0
          }
        >
          {pending.configureCf ? "Working…" : submitLabel()}
        </Button>
      )}
    </>
  );

  return (
    <div className="col gap-20">
      <PageHeader
        title="Remote access"
        description="Give Wardnet a public hostname and a valid HTTPS certificate so you can reach it securely from anywhere."
      />

      {configured && tls && (
        <Card>
          <CardHeader>
            <CardTitle>{changing ? "Change provider" : "Status"}</CardTitle>
            {!changing && (
              <CardAction className="flex gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleRecheck}
                  disabled={resolution.isFetching}
                >
                  {resolution.isFetching ? "Checking…" : "Recheck DNS"}
                </Button>
                <Button variant="outline" size="sm" onClick={openChange}>
                  Change provider
                </Button>
              </CardAction>
            )}
          </CardHeader>
          {changing ? (
            <>
              <CardContent>{formFields}</CardContent>
              <CardFooter className="justify-end gap-2">
                {formActions}
              </CardFooter>
            </>
          ) : (
            <CardContent>
              <RemoteAccessStatus
                tls={tls}
                variant="full"
                ddns={ddns}
                resolution={resolution.data}
                lastCheckedAt={resolution.dataUpdatedAt}
              />
            </CardContent>
          )}
        </Card>
      )}

      {!configured && (
        <Card>
          <CardHeader>
            <CardTitle>Enable remote access</CardTitle>
          </CardHeader>
          <CardContent>{formFields}</CardContent>
          <CardFooter className="justify-end gap-2">{formActions}</CardFooter>
        </Card>
      )}

      {configured && (
        <Card style={{ background: "var(--danger-soft)" }}>
          <CardHeader>
            <CardTitle>Remove remote access</CardTitle>
            <CardAction>
              <Button
                variant="destructive"
                onClick={() => setRemoveOpen(true)}
                disabled={teardown.isPending}
              >
                Remove remote access
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent>
            <Text as="p" size="sm" className="text-danger-soft-ink">
              Releases the public hostname, deletes the certificate, and reverts
              to plain HTTP. You can set it up again at any time.
            </Text>
          </CardContent>
        </Card>
      )}

      <AlertModal open={removeOpen} onOpenChange={setRemoveOpen}>
        <AlertModalContent>
          <AlertModalHeader>
            <AlertModalTitle>Remove remote access?</AlertModalTitle>
            <AlertModalDescription>
              The public hostname will be released and the certificate deleted;
              Wardnet reverts to plain HTTP. You can set it up again at any
              time.
            </AlertModalDescription>
          </AlertModalHeader>
          <AlertModalFooter>
            <AlertModalCancel asChild>
              <Button variant="outline" disabled={teardown.isPending}>
                Cancel
              </Button>
            </AlertModalCancel>
            <AlertModalAction asChild>
              <Button
                variant="destructive"
                onClick={handleRemove}
                disabled={teardown.isPending}
              >
                {teardown.isPending ? "Removing…" : "Remove"}
              </Button>
            </AlertModalAction>
          </AlertModalFooter>
        </AlertModalContent>
      </AlertModal>
    </div>
  );
}

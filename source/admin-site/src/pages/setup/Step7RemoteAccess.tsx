import { Button, Heading, Text } from "@wardnet/web";
import { WardnetApiError } from "@wardnet/js";
import { useAdvanceWizard, useTlsStatus } from "@wardnet/web";
import { RemoteAccessProgress } from "@/components/features/RemoteAccessProgress";
import {
  CloudflareFields,
  ProviderOption,
  WardnetFields,
} from "@/components/features/wardnet-enrollment";
import {
  useWardnetEnrollment,
  type WardnetStep,
} from "@/lib/wardnet-enrollment";
import { useState } from "react";
import { suggestName } from "@/lib/suggestName";

/**
 * Step 7 — enable remote access (HTTPS).
 *
 * Lets the operator give the gateway a public hostname and a real certificate
 * via either **wardnet** (the managed `<slug>.my.wardnet.services`, reached
 * through a one-time email enrollment) or their own **Cloudflare** domain
 * (BYOD). Registration persists synchronously; the certificate is then issued
 * in the background, so this step never blocks — the operator can wait for the
 * green "live" state, or Continue/Skip at any time. Issuance can also be retried
 * later from Settings, so an offline gateway still completes setup.
 *
 * The provider form (state machine, fields, availability check) lives in the
 * shared {@link useWardnetEnrollment} hook; this step only wires success/error
 * and renders the wizard-specific progress/skip flow.
 */
export default function Step7RemoteAccess() {
  const advance = useAdvanceWizard();

  const [formError, setFormError] = useState<string | null>(null);
  // Once provisioning has been kicked off we swap the form for live progress.
  const [started, setStarted] = useState(false);
  // Set when an attempt failed because the upstream service was unreachable (vs
  // a fixable input error) — swaps the form for a clear "service unavailable,
  // continue anyway" view.
  const [upstreamDown, setUpstreamDown] = useState(false);

  function describeError(err: unknown): string {
    if (err instanceof WardnetApiError) return err.body.error;
    return "Couldn't reach the daemon. You can skip and set this up later from Settings.";
  }

  // A 502/503 means the daemon reached out but the upstream (wardnet cloud or
  // Cloudflare) was unavailable — an outage, not a user mistake. Bad input
  // (e.g. a wrong code or rejected token) comes back as a 4xx and stays on the
  // form to fix.
  function isUpstreamDown(err: unknown): boolean {
    return (
      err instanceof WardnetApiError &&
      (err.status === 502 || err.status === 503)
    );
  }

  const enrollment = useWardnetEnrollment({
    onProvisioned: () => setStarted(true),
    onError: (err) => {
      if (isUpstreamDown(err)) {
        setUpstreamDown(true);
      } else {
        setFormError(describeError(err));
      }
    },
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
    slugDisabled,
    pending,
  } = enrollment;

  // Poll TLS status only after we've started provisioning.
  const { data: tlsStatus } = useTlsStatus({ enabled: started });

  async function finish() {
    setFormError(null);
    try {
      await advance.mutateAsync({ to_step: "completed" });
    } catch (err) {
      // Surface the failure instead of silently stalling on this step.
      setFormError(describeError(err));
    }
  }

  // ── Post-registration: live provisioning progress ────────────────────────
  if (started) {
    const phase = tlsStatus?.phase;
    return (
      <div className="flex flex-col gap-5">
        <div className="flex flex-col gap-1">
          <Heading level={2} size="3xl" className="text-ink">
            Remote access
          </Heading>
          <Text as="p" size="sm" className="mt-1 text-ink-3">
            Your hostname is registered. The certificate is being issued in the
            background — you can wait here or finish setup; it'll keep going.
          </Text>
        </div>

        {tlsStatus ? (
          <RemoteAccessProgress status={tlsStatus} />
        ) : (
          <Text
            as="div"
            size="sm"
            className="rounded-md border border-line bg-sunken p-4 text-ink-3"
          >
            Starting certificate issuance…
          </Text>
        )}

        {formError && (
          <Text as="p" size="sm" className="text-danger">
            {formError}
          </Text>
        )}

        <Button
          onClick={finish}
          disabled={advance.isPending}
          className="w-full"
        >
          {advance.isPending
            ? "Finishing…"
            : phase === "issued"
              ? "Finish"
              : "Continue (issuance keeps running)"}
        </Button>
      </div>
    );
  }

  // ── Upstream unavailable: the service is down, not the operator's input ───
  if (upstreamDown) {
    const serviceName =
      provider === "wardnet" ? "wardnet service" : "Cloudflare";
    return (
      <div className="flex flex-col gap-5">
        <div className="flex flex-col gap-1">
          <Heading level={2} size="3xl" className="text-ink">
            Remote access (HTTPS)
          </Heading>
          <Text as="p" size="sm" className="mt-1 text-ink-3">
            Wardnet couldn't reach the {serviceName} right now.
          </Text>
        </div>

        <Text
          as="div"
          size="sm"
          className="rounded-md border border-line bg-sunken p-4 text-ink-3"
        >
          This is usually temporary. You can finish setup now and enable remote
          access later from Settings — the rest of your configuration is
          unaffected.
        </Text>

        <div className="flex flex-col gap-2">
          <Button
            onClick={finish}
            disabled={advance.isPending}
            className="w-full"
          >
            {advance.isPending
              ? "Continuing…"
              : "Continue without remote access"}
          </Button>
          <Button
            variant="outline"
            onClick={() => setUpstreamDown(false)}
            disabled={advance.isPending}
            className="w-full"
          >
            Back
          </Button>
        </div>
      </div>
    );
  }

  // ── Pre-registration: provider picker + form ─────────────────────────────
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <Heading level={2} size="3xl" className="text-ink">
          Remote access (HTTPS)
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          Give Wardnet a public hostname and a valid certificate so you can
          reach it securely from anywhere. This step is optional, you can skip
          and set this up later from Settings.
        </Text>
      </div>

      <div className="flex flex-col gap-2">
        <ProviderOption
          label="Wardnet"
          description="Zero-config. We assign a hostname under wardnet.services and handle DNS — enroll with your wardnet account."
          selected={provider === "wardnet"}
          onSelect={() => setProvider("wardnet")}
        />
        <ProviderOption
          label="Your own domain (Cloudflare)"
          description="Use a domain you control via a Cloudflare API token."
          selected={provider === "cloudflare"}
          onSelect={() => setProvider("cloudflare")}
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

      <div className="flex flex-col gap-2">
        {provider === "wardnet" ? (
          <WardnetActions
            step={wardnetStep}
            slugDisabled={slugDisabled}
            sending={pending.sendCode}
            verifying={pending.verify}
            registering={pending.register}
            emailValid={email.includes("@")}
            codeValid={code.trim().length > 0}
            onSendCode={enrollment.sendCode}
            onVerifyCode={enrollment.verifyCode}
            onRegister={enrollment.registerWardnet}
          />
        ) : (
          <Button
            onClick={enrollment.enableCloudflare}
            disabled={
              pending.configureCf || domain.length === 0 || token.length === 0
            }
            className="w-full"
          >
            {pending.configureCf ? "Configuring…" : "Enable remote access"}
          </Button>
        )}
        <Button
          variant="outline"
          onClick={finish}
          disabled={advance.isPending}
          data-testid="setup-remote-access-skip"
          className="w-full"
        >
          {advance.isPending ? "Skipping…" : "Skip for now"}
        </Button>
      </div>
    </div>
  );
}

/** The wardnet action button(s), one per step. Each gates only on its own mutation. */
function WardnetActions({
  step,
  slugDisabled,
  sending,
  verifying,
  registering,
  emailValid,
  codeValid,
  onSendCode,
  onVerifyCode,
  onRegister,
}: {
  step: WardnetStep;
  slugDisabled: boolean;
  sending: boolean;
  verifying: boolean;
  registering: boolean;
  emailValid: boolean;
  codeValid: boolean;
  onSendCode: () => void;
  onVerifyCode: () => void;
  onRegister: () => void;
}) {
  if (step === "email") {
    return (
      <Button
        onClick={onSendCode}
        disabled={sending || !emailValid}
        className="w-full"
      >
        {sending ? "Sending…" : "Send code"}
      </Button>
    );
  }
  if (step === "code") {
    return (
      <Button
        onClick={onVerifyCode}
        disabled={verifying || !codeValid}
        className="w-full"
      >
        {verifying ? "Verifying…" : "Verify code"}
      </Button>
    );
  }
  return (
    <Button onClick={onRegister} disabled={slugDisabled} className="w-full">
      {registering ? "Registering…" : "Enable remote access"}
    </Button>
  );
}

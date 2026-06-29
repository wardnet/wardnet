import { useEffect, useState } from "react";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { WardnetApiError } from "@wardnet/js";
import {
  useAdvanceWizard,
  useCheckDdnsSlug,
  useConfigureCloudflare,
  useEnrollDdns,
  useRegisterDdns,
  useRequestEnrollmentCode,
  useTlsStatus,
} from "@wardnet/web";
import { RemoteAccessProgress } from "@/components/features/RemoteAccessProgress";
import { isReservedName, isValidName, suggestName } from "@/lib/suggestName";

type Provider = "wardnet" | "cloudflare";

/**
 * The wardnet path is a three-step enrollment before the slug is chosen:
 * email the account a one-time code → enter the code (enroll) → pick a slug.
 */
type WardnetStep = "email" | "code" | "slug";

type Availability =
  | "unknown"
  | "checking"
  | "available"
  | "taken"
  | "invalid"
  | "error";

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
 */
export default function Step7RemoteAccess() {
  const advance = useAdvanceWizard();
  const requestCode = useRequestEnrollmentCode();
  const enroll = useEnrollDdns();
  const register = useRegisterDdns();
  const configureCf = useConfigureCloudflare();
  // `mutateAsync` is referentially stable, so it's safe in the effect deps.
  const { mutateAsync: checkSlugAsync } = useCheckDdnsSlug();

  const [provider, setProvider] = useState<Provider>("wardnet");
  const [wardnetStep, setWardnetStep] = useState<WardnetStep>("email");
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");
  const [slug, setSlug] = useState(() => suggestName());
  const [serverAvailability, setServerAvailability] = useState<
    "unknown" | "checking" | "available" | "taken" | "error"
  >("unknown");
  const [token, setToken] = useState("");
  const [domain, setDomain] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  // Once provisioning has been kicked off we swap the form for live progress.
  const [started, setStarted] = useState(false);
  // Set when an attempt failed because the upstream service was unreachable (vs
  // a fixable input error) — swaps the form for a clear "service unavailable,
  // continue anyway" view.
  const [upstreamDown, setUpstreamDown] = useState(false);

  // Poll TLS status only after we've started provisioning.
  const { data: tlsStatus } = useTlsStatus({ enabled: started });

  // Client-side validity is derived during render (no effect/setState), so the
  // "invalid" hint and the disabled button update instantly as the user types.
  const clientValid =
    provider !== "wardnet" || wardnetStep !== "slug" || isValidName(slug);
  const availability: Availability = !clientValid
    ? "invalid"
    : serverAvailability;

  // Debounced live availability check for the wardnet slug — only once enrolled
  // and on the slug step. All state writes are async (inside the timer /
  // promise), never synchronous in the effect body.
  useEffect(() => {
    if (provider !== "wardnet" || wardnetStep !== "slug" || !isValidName(slug))
      return;
    let cancelled = false;
    const handle = setTimeout(() => {
      setServerAvailability("checking");
      checkSlugAsync(slug)
        .then((res) => {
          if (!cancelled)
            setServerAvailability(res.available ? "available" : "taken");
        })
        .catch(() => {
          // The daemon couldn't reach the cloud (offline / service down).
          // Surface it rather than leaving the field with no feedback.
          if (!cancelled) setServerAvailability("error");
        });
    }, 400);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [slug, provider, wardnetStep, checkSlugAsync]);

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

  function handleEnableError(err: unknown) {
    if (isUpstreamDown(err)) {
      setUpstreamDown(true);
    } else {
      setFormError(describeError(err));
    }
  }

  async function handleSendCode() {
    setFormError(null);
    try {
      await requestCode.mutateAsync({ email });
      setWardnetStep("code");
    } catch (err) {
      handleEnableError(err);
    }
  }

  async function handleVerifyCode() {
    setFormError(null);
    try {
      await enroll.mutateAsync({ code });
      setWardnetStep("slug");
    } catch (err) {
      handleEnableError(err);
    }
  }

  async function handleRegisterWardnet() {
    setFormError(null);
    try {
      await register.mutateAsync({ slug });
      setStarted(true);
    } catch (err) {
      handleEnableError(err);
    }
  }

  async function handleEnableCloudflare() {
    setFormError(null);
    try {
      await configureCf.mutateAsync({ token, domain });
      setStarted(true);
    } catch (err) {
      handleEnableError(err);
    }
  }

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
          <h2 className="h-title">Remote access</h2>
          <p className="h-sub">
            Your hostname is registered. The certificate is being issued in the
            background — you can wait here or finish setup; it'll keep going.
          </p>
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
          <h2 className="h-title">Remote access (HTTPS)</h2>
          <p className="h-sub">
            Wardnet couldn't reach the {serviceName} right now.
          </p>
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
  const busy =
    requestCode.isPending ||
    enroll.isPending ||
    register.isPending ||
    configureCf.isPending;
  const slugDisabled =
    busy || availability === "checking" || availability === "invalid";

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="h-title">Remote access (HTTPS)</h2>
        <p className="h-sub">
          Give Wardnet a public hostname and a valid certificate so you can
          reach it securely from anywhere. This step is optional, you can skip
          and set this up later from Settings.
        </p>
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
        <WardnetForm
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
        <div className="flex flex-col gap-4">
          <Field label="Domain" htmlFor="cf-domain" name="cf-domain">
            <Input
              id="cf-domain"
              value={domain}
              onChange={(e) => setDomain(e.target.value.toLowerCase())}
              placeholder="home.example.com"
              autoComplete="off"
            />
          </Field>
          <Field
            label="Cloudflare API token"
            htmlFor="cf-token"
            name="cf-token"
          >
            <Input
              id="cf-token"
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="DNS:Edit token for the zone"
              autoComplete="off"
            />
          </Field>
        </div>
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
            busy={busy}
            slugDisabled={slugDisabled}
            sending={requestCode.isPending}
            verifying={enroll.isPending}
            registering={register.isPending}
            emailValid={email.includes("@")}
            codeValid={code.trim().length > 0}
            onSendCode={handleSendCode}
            onVerifyCode={handleVerifyCode}
            onRegister={handleRegisterWardnet}
          />
        ) : (
          <Button
            onClick={handleEnableCloudflare}
            disabled={busy || domain.length === 0 || token.length === 0}
            className="w-full"
          >
            {configureCf.isPending ? "Configuring…" : "Enable remote access"}
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

/** The wardnet enrollment form, switching fields by step. */
function WardnetForm({
  step,
  email,
  onEmailChange,
  code,
  onCodeChange,
  slug,
  onSlugChange,
  availability,
  onSuggest,
  onChangeEmail,
}: {
  step: WardnetStep;
  email: string;
  onEmailChange: (v: string) => void;
  code: string;
  onCodeChange: (v: string) => void;
  slug: string;
  onSlugChange: (v: string) => void;
  availability: Availability;
  onSuggest: () => void;
  onChangeEmail: () => void;
}) {
  if (step === "email") {
    return (
      <Field
        label="Wardnet account email"
        htmlFor="wardnet-email"
        name="wardnet-email"
      >
        <Input
          id="wardnet-email"
          type="email"
          value={email}
          onChange={(e) => onEmailChange(e.target.value.trim())}
          placeholder="you@example.com"
          autoComplete="email"
        />
      </Field>
    );
  }

  if (step === "code") {
    return (
      <div className="flex flex-col gap-2">
        <Field
          label="Enrollment code"
          htmlFor="wardnet-code"
          name="wardnet-code"
        >
          <Input
            id="wardnet-code"
            value={code}
            onChange={(e) => onCodeChange(e.target.value.trim())}
            placeholder="Code from your email"
            autoComplete="one-time-code"
          />
        </Field>
        <Text as="div" size="xs" className="flex items-center justify-between">
          <span className="text-ink-3">
            We emailed a one-time code to {email}.
          </span>
          <button
            type="button"
            className="text-accent hover:underline"
            onClick={onChangeEmail}
          >
            Change email
          </button>
        </Text>
      </div>
    );
  }

  // step === "slug"
  return (
    <div className="flex flex-col gap-2">
      <Field label="Hostname" htmlFor="ddns-slug" name="ddns-slug">
        <Input
          id="ddns-slug"
          value={slug}
          onChange={(e) => onSlugChange(e.target.value.toLowerCase())}
          placeholder="happy-einstein"
          autoComplete="off"
        />
      </Field>
      <Text as="div" size="xs" className="flex items-center justify-between">
        <AvailabilityHint availability={availability} slug={slug} />
        <button
          type="button"
          className="text-accent hover:underline"
          onClick={onSuggest}
        >
          Suggest another
        </button>
      </Text>
    </div>
  );
}

/** The wardnet action button(s), one per step. */
function WardnetActions({
  step,
  busy,
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
  busy: boolean;
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
        disabled={busy || !emailValid}
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
        disabled={busy || !codeValid}
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

function ProviderOption({
  label,
  description,
  selected,
  onSelect,
}: {
  label: string;
  description: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 rounded-md border border-line p-3 hover:bg-sunken">
      <input
        type="radio"
        name="ddns-provider"
        checked={selected}
        onChange={onSelect}
        className="mt-1"
      />
      <span className="flex flex-col gap-0.5">
        <Text as="span" size="sm" weight="medium" className="text-ink">
          {label}
        </Text>
        <Text as="span" size="xs" className="text-ink-3">
          {description}
        </Text>
      </span>
    </label>
  );
}

function AvailabilityHint({
  availability,
  slug,
}: {
  availability: Availability;
  slug: string;
}) {
  switch (availability) {
    case "invalid":
      return (
        <span className="text-ink-3">
          {slug.length < 3
            ? "At least 3 characters."
            : isReservedName(slug)
              ? "This name is reserved — try another."
              : "Use lowercase letters, digits, and hyphens only."}
        </span>
      );
    case "checking":
      return <span className="text-ink-3">Checking availability…</span>;
    case "available":
      return <span className="text-ink-2">✓ {slug} is available</span>;
    case "taken":
      return <span className="text-danger">{slug} is taken — try another</span>;
    case "error":
      return (
        <span className="text-ink-3">
          Couldn't check availability — you can still continue or try again.
        </span>
      );
    default:
      return <span className="text-ink-3">&nbsp;</span>;
  }
}

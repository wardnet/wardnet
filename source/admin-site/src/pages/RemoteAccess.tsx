import { useEffect, useState } from "react";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
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
import { toast } from "sonner";
import { WardnetApiError } from "@wardnet/js";
import {
  useCheckDdnsSlug,
  useConfigureCloudflare,
  useDdnsStatus,
  useDeleteDdns,
  useEnrollDdns,
  useRegisterDdns,
  useRequestEnrollmentCode,
  useResolutionCheck,
  useTlsStatus,
} from "@wardnet/web";
import { PageHeader } from "@/components/compound/PageHeader";
import { RemoteAccessStatus } from "@/components/features/RemoteAccessStatus";
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
 * Remote access settings (admin only) — configure, switch, and monitor the
 * daemon-owned HTTPS provisioning the setup wizard introduces. Unlike the
 * wizard's one-shot step this page is the steady-state surface: it leads with
 * live status (phase, certificate, resolution check) and keeps the provider
 * form behind a "Change provider" action. The wardnet path runs the same
 * email → code → slug enrollment as the wizard; enabling and switching reuse the
 * same `POST /api/ddns/{enrollment-code,enroll,register,cloudflare}` endpoints
 * (the daemon performs the new-first/teardown-old switch and kicks off
 * background issuance), so this page only drives them and polls
 * `GET /api/tls/status`.
 */
export default function RemoteAccess() {
  const { data: ddns } = useDdnsStatus();
  const { data: tls } = useTlsStatus();
  const configured = !!ddns?.provider;
  const resolution = useResolutionCheck(configured);

  const requestCode = useRequestEnrollmentCode();
  const enroll = useEnrollDdns();
  const register = useRegisterDdns();
  const configureCf = useConfigureCloudflare();
  const teardown = useDeleteDdns();
  const { mutateAsync: checkSlugAsync } = useCheckDdnsSlug();

  // The provider form is shown when enabling (unconfigured) or after the
  // operator explicitly chooses to change an existing configuration.
  const [changing, setChanging] = useState(false);
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
  const [removeOpen, setRemoveOpen] = useState(false);

  const clientValid =
    provider !== "wardnet" || wardnetStep !== "slug" || isValidName(slug);
  const availability: Availability = !clientValid
    ? "invalid"
    : serverAvailability;

  // Debounced live availability check for the wardnet slug (mirrors the wizard),
  // only once enrolled and on the slug step.
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
    return "Couldn't reach the daemon. Please try again.";
  }

  function selectProvider(next: Provider) {
    setFormError(null);
    setProvider(next);
    if (next === "wardnet") setWardnetStep("email");
  }

  function openChange() {
    setFormError(null);
    setProvider(ddns?.provider === "cloudflare" ? "cloudflare" : "wardnet");
    setWardnetStep("email");
    setEmail("");
    setCode("");
    setSlug(suggestName());
    setToken("");
    setDomain("");
    setChanging(true);
  }

  async function handleSendCode() {
    setFormError(null);
    try {
      await requestCode.mutateAsync({ email });
      setWardnetStep("code");
    } catch (err) {
      setFormError(describeError(err));
    }
  }

  async function handleVerifyCode() {
    setFormError(null);
    try {
      await enroll.mutateAsync({ code });
      setWardnetStep("slug");
    } catch (err) {
      setFormError(describeError(err));
    }
  }

  async function handleRegisterWardnet() {
    setFormError(null);
    try {
      await register.mutateAsync({ slug });
      setChanging(false);
    } catch (err) {
      setFormError(describeError(err));
    }
  }

  async function handleEnableCloudflare() {
    setFormError(null);
    try {
      await configureCf.mutateAsync({ token, domain });
      setChanging(false);
    } catch (err) {
      setFormError(describeError(err));
    }
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

  const busy =
    requestCode.isPending ||
    enroll.isPending ||
    register.isPending ||
    configureCf.isPending;
  const slugDisabled =
    busy || availability === "checking" || availability === "invalid";

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
    </div>
  );

  // Footer actions for the provider form (Cancel only when changing an existing config).
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
            onClick={handleSendCode}
            disabled={busy || !email.includes("@")}
          >
            {requestCode.isPending ? "Sending…" : "Send code"}
          </Button>
        ) : wardnetStep === "code" ? (
          <Button
            onClick={handleVerifyCode}
            disabled={busy || code.trim().length === 0}
          >
            {enroll.isPending ? "Verifying…" : "Verify code"}
          </Button>
        ) : (
          <Button onClick={handleRegisterWardnet} disabled={slugDisabled}>
            {register.isPending ? "Working…" : submitLabel()}
          </Button>
        )
      ) : (
        <Button
          onClick={handleEnableCloudflare}
          disabled={busy || domain.length === 0 || token.length === 0}
        >
          {configureCf.isPending ? "Working…" : submitLabel()}
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

/** The wardnet enrollment fields, switching by step. */
function WardnetFields({
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
        <Text size="sm" weight="medium" className="text-ink">
          {label}
        </Text>
        <Text size="xs" className="text-ink-3">
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
          Couldn't check availability — you can still continue.
        </span>
      );
    default:
      return <span className="text-ink-3">&nbsp;</span>;
  }
}

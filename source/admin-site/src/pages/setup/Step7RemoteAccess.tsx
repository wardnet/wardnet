import { useEffect, useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { WardnetApiError } from "@wardnet/js";
import {
  useAdvanceWizard,
  useCheckDdnsName,
  useConfigureCloudflare,
  useRegisterDdns,
  useTlsStatus,
} from "@wardnet/wardnet-web";
import { RemoteAccessProgress } from "@/components/features/RemoteAccessProgress";
import { isReservedName, isValidName, suggestName } from "@/lib/suggestName";

type Provider = "bridge" | "cloudflare";
type Availability = "unknown" | "checking" | "available" | "taken" | "invalid" | "error";

/**
 * Step 7 — enable remote access (HTTPS).
 *
 * Lets the operator give the Pi a public hostname and a real certificate via
 * either the wardnet bridge (default, zero-config) or their own Cloudflare
 * domain (BYOD). Registration persists synchronously; the certificate is then
 * issued in the background, so this step never blocks — the operator can wait
 * for the green "live" state, or Continue/Skip at any time. Issuance can also
 * be retried later from Settings, so an offline Pi still completes setup.
 */
export default function Step7RemoteAccess() {
  const advance = useAdvanceWizard();
  const register = useRegisterDdns();
  const configureCf = useConfigureCloudflare();
  // `mutateAsync` is referentially stable, so it's safe in the effect deps.
  const { mutateAsync: checkNameAsync } = useCheckDdnsName();

  const [provider, setProvider] = useState<Provider>("bridge");
  const [name, setName] = useState(() => suggestName());
  const [serverAvailability, setServerAvailability] = useState<
    "unknown" | "checking" | "available" | "taken" | "error"
  >("unknown");
  const [token, setToken] = useState("");
  const [domain, setDomain] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  // Once provisioning has been kicked off we swap the form for live progress.
  const [started, setStarted] = useState(false);
  // Set when an Enable attempt failed because the upstream service was
  // unreachable (vs a fixable input error) — swaps the form for a clear
  // "service unavailable, continue anyway" view.
  const [upstreamDown, setUpstreamDown] = useState(false);

  // Poll TLS status only after we've started provisioning.
  const { data: tlsStatus } = useTlsStatus({ enabled: started });

  // Client-side validity is derived during render (no effect/setState), so the
  // "invalid" hint and the disabled button update instantly as the user types.
  const clientValid = provider !== "bridge" || isValidName(name);
  const availability: Availability = !clientValid ? "invalid" : serverAvailability;

  // Debounced live availability check for the bridge name. All state writes are
  // async (inside the timer / promise), never synchronous in the effect body.
  useEffect(() => {
    if (provider !== "bridge" || !isValidName(name)) return;
    let cancelled = false;
    const handle = setTimeout(() => {
      setServerAvailability("checking");
      checkNameAsync(name)
        .then((res) => {
          if (!cancelled) setServerAvailability(res.available ? "available" : "taken");
        })
        .catch(() => {
          // The daemon couldn't reach a bridge (offline / bridge down). Surface
          // it rather than leaving the field with no feedback.
          if (!cancelled) setServerAvailability("error");
        });
    }, 400);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [name, provider, checkNameAsync]);

  function describeError(err: unknown): string {
    if (err instanceof WardnetApiError) return err.body.error;
    return "Couldn't reach the daemon. You can skip and set this up later from Settings.";
  }

  // A 502/503 means the daemon reached out but the upstream (bridge or
  // Cloudflare) was unavailable — an outage, not a user mistake. Bad input
  // (e.g. a rejected token) comes back as a 4xx and stays on the form to fix.
  function isUpstreamDown(err: unknown): boolean {
    return err instanceof WardnetApiError && (err.status === 502 || err.status === 503);
  }

  function handleEnableError(err: unknown) {
    if (isUpstreamDown(err)) {
      setUpstreamDown(true);
    } else {
      setFormError(describeError(err));
    }
  }

  async function handleEnableBridge() {
    setFormError(null);
    try {
      await register.mutateAsync({ name });
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
            Your hostname is registered. The certificate is being issued in the background — you can
            wait here or finish setup; it'll keep going.
          </p>
        </div>

        {tlsStatus ? (
          <RemoteAccessProgress status={tlsStatus} />
        ) : (
          <div className="rounded-md border border-line bg-sunken p-4 text-sm text-ink-3">
            Starting certificate issuance…
          </div>
        )}

        {formError && <p className="text-sm text-danger">{formError}</p>}

        <Button onClick={finish} disabled={advance.isPending} className="w-full">
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
    const serviceName = provider === "bridge" ? "hostname service" : "Cloudflare";
    return (
      <div className="flex flex-col gap-5">
        <div className="flex flex-col gap-1">
          <h2 className="h-title">Remote access (HTTPS)</h2>
          <p className="h-sub">Wardnet couldn't reach the {serviceName} right now.</p>
        </div>

        <div className="rounded-md border border-line bg-sunken p-4 text-sm text-ink-3">
          This is usually temporary. You can finish setup now and enable remote access later from
          Settings — the rest of your configuration is unaffected.
        </div>

        <div className="flex flex-col gap-2">
          <Button onClick={finish} disabled={advance.isPending} className="w-full">
            {advance.isPending ? "Continuing…" : "Continue without remote access"}
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
  const busy = register.isPending || configureCf.isPending;
  const bridgeDisabled = busy || availability === "checking" || availability === "invalid";

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="h-title">Remote access (HTTPS)</h2>
        <p className="h-sub">
          Give Wardnet a public hostname and a valid certificate so you can reach it securely from
          anywhere. This step is optional, you can skip and set this up later from Settings.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <ProviderOption
          label="Wardnet bridge"
          description="Zero-config. We assign a hostname under wardnet.services and handle DNS."
          selected={provider === "bridge"}
          onSelect={() => setProvider("bridge")}
        />
        <ProviderOption
          label="Your own domain (Cloudflare)"
          description="Use a domain you control via a Cloudflare API token."
          selected={provider === "cloudflare"}
          onSelect={() => setProvider("cloudflare")}
        />
      </div>

      {provider === "bridge" ? (
        <div className="flex flex-col gap-2">
          <Field label="Hostname" htmlFor="ddns-name" name="ddns-name">
            <Input
              id="ddns-name"
              value={name}
              onChange={(e) => setName(e.target.value.toLowerCase())}
              placeholder="happy-einstein"
              autoComplete="off"
            />
          </Field>
          <div className="flex items-center justify-between text-xs">
            <AvailabilityHint availability={availability} name={name} />
            <button
              type="button"
              className="text-accent hover:underline"
              onClick={() => setName(suggestName())}
            >
              Suggest another
            </button>
          </div>
        </div>
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
          <Field label="Cloudflare API token" htmlFor="cf-token" name="cf-token">
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

      {formError && <p className="text-sm text-danger">{formError}</p>}

      <div className="flex flex-col gap-2">
        {provider === "bridge" ? (
          <Button onClick={handleEnableBridge} disabled={bridgeDisabled} className="w-full">
            {register.isPending ? "Registering…" : "Enable remote access"}
          </Button>
        ) : (
          <Button
            onClick={handleEnableCloudflare}
            disabled={busy || domain.length === 0 || token.length === 0}
            className="w-full"
          >
            {configureCf.isPending ? "Configuring…" : "Enable remote access"}
          </Button>
        )}
        <Button variant="outline" onClick={finish} disabled={advance.isPending} className="w-full">
          {advance.isPending ? "Skipping…" : "Skip for now"}
        </Button>
      </div>
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
        <span className="text-sm font-medium text-ink">{label}</span>
        <span className="text-xs text-ink-3">{description}</span>
      </span>
    </label>
  );
}

function AvailabilityHint({ availability, name }: { availability: Availability; name: string }) {
  switch (availability) {
    case "invalid":
      return (
        <span className="text-ink-3">
          {name.length < 3
            ? "At least 3 characters."
            : isReservedName(name)
              ? "This name is reserved — try another."
              : "Use lowercase letters, digits, and hyphens only."}
        </span>
      );
    case "checking":
      return <span className="text-ink-3">Checking availability…</span>;
    case "available":
      return <span className="text-ink-2">✓ {name} is available</span>;
    case "taken":
      return <span className="text-danger">{name} is taken — try another</span>;
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

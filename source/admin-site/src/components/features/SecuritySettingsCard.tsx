import { useState } from "react";

import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Field,
  FormActions,
  Input,
  Toggle,
  useDnsConfig,
  useUpdateDnsConfig,
} from "@wardnet/web";

/** DNS security settings (Stage 4): DNSSEC validation, rebinding
 *  protection, and per-client rate limiting. Toggles save immediately;
 *  the rate limit uses an explicit Save so transient keystrokes don't
 *  thrash the config. */
export function SecuritySettingsCard() {
  const { data, isLoading } = useDnsConfig();
  const update = useUpdateDnsConfig();
  const config = data?.config;

  const dnssec = config?.dnssec_enabled ?? false;
  const rebinding = config?.rebinding_protection ?? true;
  const currentRate = config?.rate_limit_per_second ?? 0;

  // Local edit buffer for the rate limit; `null` means "in sync with
  // the loaded config".
  const [rate, setRate] = useState<string | null>(null);
  const rateValue = rate ?? String(currentRate);
  const rateParsed = Number(rateValue);
  const rateValid =
    rateValue.trim() !== "" && Number.isInteger(rateParsed) && rateParsed >= 0;
  const rateDirty = rate !== null && rateParsed !== currentRate;
  const busy = isLoading || update.isPending;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Security</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <Field
          direction="row"
          label="DNSSEC validation"
          htmlFor="dns-dnssec"
          help="Cryptographically validate upstream answers. May break domains with misconfigured DNSSEC."
        >
          <Toggle
            id="dns-dnssec"
            aria-label="Enable DNSSEC validation"
            checked={dnssec}
            disabled={busy}
            onCheckedChange={(next) => update.mutate({ dnssec_enabled: next })}
          />
        </Field>

        <Field
          direction="row"
          label="Rebinding protection"
          htmlFor="dns-rebinding"
          help="Reject answers for public domains that resolve to private/internal IP addresses."
        >
          <Toggle
            id="dns-rebinding"
            aria-label="Enable DNS rebinding protection"
            checked={rebinding}
            disabled={busy}
            onCheckedChange={(next) =>
              update.mutate({ rebinding_protection: next })
            }
          />
        </Field>

        <Field
          label="Rate limit"
          htmlFor="dns-rate-limit"
          help="Max queries per second per client. 0 disables rate limiting."
          error={
            rate !== null && !rateValid
              ? "Enter a whole number ≥ 0."
              : undefined
          }
        >
          <Input
            id="dns-rate-limit"
            type="number"
            min={0}
            value={rateValue}
            disabled={busy}
            onChange={(e) => setRate(e.target.value)}
          />
        </Field>
      </CardContent>
      {rateDirty && (
        <FormActions
          secondaryLabel="Cancel"
          secondaryProps={{ onClick: () => setRate(null), disabled: busy }}
          primaryLabel="Save"
          primaryProps={{
            disabled: busy || !rateValid,
            onClick: () =>
              update.mutate(
                { rate_limit_per_second: rateParsed },
                { onSuccess: () => setRate(null) },
              ),
          }}
        />
      )}
    </Card>
  );
}

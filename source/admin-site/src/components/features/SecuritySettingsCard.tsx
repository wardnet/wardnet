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
} from "@wardnet/web";
import type { DnsConfig, UpdateDnsConfigRequest } from "@wardnet/js";

interface SecuritySettingsCardProps {
  /** The DNS config, or `undefined` while the page's query loads. */
  config: DnsConfig | undefined;
  isLoading: boolean;
  /** Matches TanStack's `mutate` signature so the page can pass the
   *  mutation's `mutate` straight through; the card uses `onSuccess` to
   *  clear its rate-limit edit buffer. */
  onUpdate: (
    body: UpdateDnsConfigRequest,
    callbacks?: { onSuccess?: () => void },
  ) => void;
  /** True while the page's update mutation is in flight. */
  updatePending: boolean;
}

/** DNS security settings (Stage 4): DNSSEC validation, rebinding
 *  protection, and per-client rate limiting. Toggles save immediately;
 *  the rate limit uses an explicit Save so transient keystrokes don't
 *  thrash the config. Pure presentation — the owning page wires the
 *  query/mutation hooks and passes data + callbacks in. */
export function SecuritySettingsCard({
  config,
  isLoading,
  onUpdate,
  updatePending,
}: SecuritySettingsCardProps) {
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
  const busy = isLoading || updatePending;

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
            onCheckedChange={(next) => onUpdate({ dnssec_enabled: next })}
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
            onCheckedChange={(next) => onUpdate({ rebinding_protection: next })}
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
              onUpdate(
                { rate_limit_per_second: rateParsed },
                { onSuccess: () => setRate(null) },
              ),
          }}
        />
      )}
    </Card>
  );
}

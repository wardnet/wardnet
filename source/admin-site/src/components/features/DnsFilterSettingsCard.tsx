import { useEffect, useState } from "react";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import type { DnsFilterConfig } from "@wardnet/js";

interface DnsFilterSettingsCardProps {
  /** Current global filter config, or `undefined` while it loads. */
  config: DnsFilterConfig | undefined;
  /** Disables the controls while the config loads or an update is in flight. */
  isLoading: boolean;
  /** Fired with the requested state when the kill switch is toggled. */
  onToggle: (enabled: boolean) => void;
  /** Fired with the new threshold when the admin saves it. */
  onThresholdSave: (threshold: number) => void;
}

/** System-level DNS filter card — the global kill switch.
 *
 *  Presentational: the owning page holds the `useDnsFilterConfig` /
 *  `useUpdateDnsFilterConfig` hooks and passes state + callback down, so the
 *  kill switch and the per-row Default toggles share one mutation instance
 *  instead of racing two independent read-modify-writes.
 *
 *  Chrome matches DhcpStatusCard / DnsServer: title + status pill +
 *  Toggle in CardAction. Profile management lives in the table below
 *  (one place for "what profiles exist"); per-device assignment lives
 *  on the device detail page. */
export function DnsFilterSettingsCard({
  config,
  isLoading,
  onToggle,
  onThresholdSave,
}: DnsFilterSettingsCardProps) {
  const enabled = config?.enabled ?? false;
  const savedThreshold = config?.blocklist_failure_alert_threshold;

  // `null` means "in sync with the server". The toggle saves immediately;
  // a number needs an explicit Save, so it gets a local edit buffer.
  const [draft, setDraft] = useState<string | null>(null);
  useEffect(() => {
    setDraft(null);
  }, [savedThreshold]);

  const value = draft ?? String(savedThreshold ?? "");
  const parsed = Number(value);
  const valid = value.trim() !== "" && Number.isInteger(parsed) && parsed >= 0;
  const dirty = draft !== null && parsed !== savedThreshold;

  return (
    <Card>
      <CardHeader>
        <CardTitle>DNS filtering</CardTitle>
        <Pill variant={enabled ? "ok" : "ghost"}>
          <span className="dot" />
          {enabled ? "Enabled" : "Disabled"}
        </Pill>
        <CardAction>
          <Toggle
            id="filter-enabled"
            aria-label="Enable DNS filtering"
            checked={enabled}
            disabled={isLoading}
            onCheckedChange={onToggle}
          />
        </CardAction>
      </CardHeader>
      <CardContent className="col gap-4">
        <Text as="p" size="sm" className="text-ink-3">
          Emergency stop. When off, every DNS query bypasses filtering
          regardless of per-device or per-profile settings.
        </Text>

        <Field
          label="Alert after consecutive refresh failures"
          hint="How many times a blocklist must fail to refresh before you are notified. A failing list keeps serving its last good download, so this is about being told it has gone stale. 0 turns the alert off."
        >
          <Input
            id="blocklist-failure-alert-threshold"
            data-testid="blocklist-failure-alert-threshold"
            type="number"
            min={0}
            value={value}
            disabled={isLoading}
            onChange={(e) => setDraft(e.target.value)}
          />
        </Field>

        {dirty && (
          <FormActions>
            <Button
              variant="ghost"
              onClick={() => setDraft(null)}
              disabled={isLoading}
            >
              Cancel
            </Button>
            <Button
              data-testid="save-blocklist-failure-alert-threshold"
              disabled={!valid || isLoading}
              onClick={() => onThresholdSave(parsed)}
            >
              Save
            </Button>
          </FormActions>
        )}
      </CardContent>
    </Card>
  );
}

import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import type { DnsFilterConfig } from "@wardnet/js";

interface DnsFilterSettingsCardProps {
  /** Current global filter config, or `undefined` while it loads. */
  config: DnsFilterConfig | undefined;
  /** Disables the toggle while the config loads or an update is in flight. */
  isLoading: boolean;
  /** Fired with the requested state when the kill switch is toggled. */
  onToggle: (enabled: boolean) => void;
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
}: DnsFilterSettingsCardProps) {
  const enabled = config?.enabled ?? false;

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
      <CardContent>
        <Text as="p" size="sm" className="text-ink-3">
          Emergency stop. When off, every DNS query bypasses filtering
          regardless of per-device or per-profile settings.
        </Text>
      </CardContent>
    </Card>
  );
}

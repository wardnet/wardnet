import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Pill } from "@wardnet/forge-web/pill";
import { Toggle } from "@wardnet/forge-web/toggle";
import { useDnsFilterConfig, useUpdateDnsFilterConfig } from "@/hooks/useDnsFilter";

/** System-level DNS filter card — the global kill switch.
 *
 *  Chrome matches DhcpStatusCard / DnsServer: title + status pill +
 *  Toggle in CardAction. Profile management lives in the table below
 *  (one place for "what profiles exist"); per-device assignment lives
 *  on the device detail page. */
export function DnsFilterSettingsCard() {
  const { data: configData, isLoading } = useDnsFilterConfig();
  const update = useUpdateDnsFilterConfig();

  const config = configData?.config;
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
            disabled={isLoading || update.isPending}
            onCheckedChange={(next) => update.mutate({ enabled: next })}
          />
        </CardAction>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-ink-3">
          Emergency stop. When off, every DNS query bypasses filtering regardless of per-device or
          per-profile settings.
        </p>
      </CardContent>
    </Card>
  );
}

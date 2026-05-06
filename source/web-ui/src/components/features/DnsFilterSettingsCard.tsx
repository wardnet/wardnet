import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Label } from "@/components/core/ui/label";
import { Switch } from "@/components/core/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import {
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useUpdateDnsFilterConfig,
} from "@/hooks/useDnsFilter";

const NONE_VALUE = "__none__";

/** Settings-page card for the global DNS filter config (kill switch + default profile). */
export function DnsFilterSettingsCard() {
  const { data: configData, isLoading } = useDnsFilterConfig();
  const { data: profilesData } = useDnsFilterProfiles();
  const update = useUpdateDnsFilterConfig();

  const config = configData?.config;
  const profiles = profilesData?.profiles ?? [];

  return (
    <Card>
      <CardHeader>
        <CardTitle>DNS filtering</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {isLoading || !config ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : (
          <>
            <div className="flex items-center justify-between gap-4">
              <div>
                <Label htmlFor="filter-enabled">DNS filtering enabled</Label>
                <p className="text-xs text-muted-foreground">
                  Emergency stop. When off, every DNS query bypasses filtering regardless of
                  per-device or per-profile settings.
                </p>
              </div>
              <Switch
                id="filter-enabled"
                checked={config.enabled}
                disabled={update.isPending}
                onCheckedChange={(enabled) => update.mutate({ enabled })}
              />
            </div>

            <div className="flex flex-col gap-2">
              <Label htmlFor="default-profile">Default profile</Label>
              <p className="text-xs text-muted-foreground">
                Applied to devices that have no explicit profile assignment. Choose "None" to leave
                unassigned devices unfiltered.
              </p>
              <Select
                value={config.default_profile_id ?? NONE_VALUE}
                onValueChange={(v) =>
                  update.mutate({ default_profile_id: v === NONE_VALUE ? null : v })
                }
              >
                <SelectTrigger id="default-profile" className="max-w-sm">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NONE_VALUE}>None (unfiltered)</SelectItem>
                  {profiles.map((p) => (
                    <SelectItem key={p.id} value={p.id}>
                      {p.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}

import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Label } from "@/components/core/ui/label";
import { Switch } from "@/components/core/ui/switch";
import { ProfileToggleList } from "@/components/compound/ProfileToggleList";
import {
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useUpdateDnsFilterConfig,
} from "@/hooks/useDnsFilter";

const DEFAULT_PROFILES_LABEL_ID = "dns-filter-default-profiles-label";

/** Settings-page card for the global DNS filter config (kill switch + default profiles). */
export function DnsFilterSettingsCard() {
  const { data: configData, isLoading: configLoading } = useDnsFilterConfig();
  const { data: profilesData, isLoading: profilesLoading } = useDnsFilterProfiles();
  const update = useUpdateDnsFilterConfig();

  const config = configData?.config;
  const profiles = profilesData?.profiles;

  // Both queries must be settled before rendering the picker — otherwise the
  // composite renders "No profiles defined." in the brief window before
  // profiles arrive.
  const ready = !configLoading && !profilesLoading && config && profiles;

  return (
    <Card>
      <CardHeader>
        <CardTitle>DNS filtering</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {!ready ? (
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

            {config.enabled && (
              <div className="flex flex-col gap-2">
                <Label id={DEFAULT_PROFILES_LABEL_ID}>Default profiles</Label>
                <p className="text-xs text-muted-foreground">
                  Applied to devices that have no explicit profile assignment. Multiple profiles
                  stack — a domain blocked in any one of them is blocked. Leave empty to leave
                  unassigned devices unfiltered.
                </p>
                <ProfileToggleList
                  profiles={profiles}
                  selectedIds={config.default_profile_ids}
                  onChange={(ids) => update.mutate({ default_profile_ids: ids })}
                  disabled={update.isPending}
                  ariaLabelledBy={DEFAULT_PROFILES_LABEL_ID}
                />
              </div>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}

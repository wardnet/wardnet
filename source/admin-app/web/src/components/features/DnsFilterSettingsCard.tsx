import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Toggle } from "@wardnet/forge-web/toggle";
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
            <Field
              direction="row"
              label="DNS filtering enabled"
              htmlFor="filter-enabled"
              help="Emergency stop. When off, every DNS query bypasses filtering regardless of per-device or per-profile settings."
            >
              <Toggle
                id="filter-enabled"
                checked={config.enabled}
                disabled={update.isPending}
                onCheckedChange={(enabled) => update.mutate({ enabled })}
              />
            </Field>

            {config.enabled && (
              <Field label="Default profiles" labelId={DEFAULT_PROFILES_LABEL_ID}>
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
              </Field>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}

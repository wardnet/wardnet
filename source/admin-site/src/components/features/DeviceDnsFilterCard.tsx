import { useState } from "react";
import { Button } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { ApiErrorAlert } from "@wardnet/web";
import { ProfileToggleList } from "@/components/compound/ProfileToggleList";
import {
  useDeviceFilterSettings,
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useUpdateDeviceFilterSettings,
} from "@wardnet/web";
import type { Device, DnsFilterProfile } from "@wardnet/js";

interface DeviceDnsFilterCardProps {
  device: Device;
}

const PROFILES_LABEL_ID = "device-dns-filter-profiles-label";

/** Editable DNS filter settings card for the device detail page —
 *  edit-mode card protocol per DhcpConfigCard / DeviceSettingsCard
 *  (folded edit form, hook-coupled via `useUpdateDeviceFilterSettings`). */
export function DeviceDnsFilterCard({ device }: DeviceDnsFilterCardProps) {
  const { data: settingsData, isLoading: settingsLoading } =
    useDeviceFilterSettings(device.id);
  const { data: profilesData, isLoading: profilesLoading } =
    useDnsFilterProfiles();
  const { data: configData, isLoading: configLoading } = useDnsFilterConfig();
  const update = useUpdateDeviceFilterSettings();

  const profiles = profilesData?.profiles;
  const settings = settingsData?.settings;
  const config = configData?.config;

  const [editing, setEditing] = useState(false);
  const [enabled, setEnabled] = useState(true);
  const [profileIds, setProfileIds] = useState<string[]>([]);

  function startEdit() {
    if (!settings) return;
    setEnabled(settings.enabled);
    setProfileIds(settings.profile_ids);
    update.reset();
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    update.reset();
  }

  async function handleSave() {
    await update.mutateAsync({
      id: device.id,
      body: { enabled, profile_ids: profileIds },
    });
    setEditing(false);
  }

  // All three queries must settle before rendering — otherwise the
  // read-only summary line resolves `defaultProfiles`/`assignedProfiles`
  // against an empty profiles cache and falsely reports "None".
  const ready =
    !settingsLoading &&
    !profilesLoading &&
    !configLoading &&
    settings &&
    profiles &&
    config;

  if (!ready) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>DNS filtering</CardTitle>
        </CardHeader>
        <CardContent>
          <Text as="p" size="sm" className="text-ink-3">
            Loading…
          </Text>
        </CardContent>
      </Card>
    );
  }

  const defaultProfiles = profiles.filter((p) =>
    config.default_profile_ids.includes(p.id),
  );
  const assignedProfiles = profiles.filter((p) =>
    settings.profile_ids.includes(p.id),
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>DNS filtering</CardTitle>
        {!editing && (
          <CardAction>
            <Button variant="outline" size="sm" onClick={startEdit}>
              Edit
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {editing ? (
        <>
          <CardContent className="flex flex-col gap-5">
            <Field
              direction="row"
              label="DNS filtering"
              htmlFor="dns-filter-enabled"
              help={
                'Off skips filtering entirely; queries that would be blocked are recorded as "blocked (skipped)" in the DNS log.'
              }
            >
              <Toggle
                id="dns-filter-enabled"
                checked={enabled}
                onCheckedChange={setEnabled}
              />
            </Field>

            <Field label="Profiles" labelId={PROFILES_LABEL_ID}>
              <DefaultProfileHint
                enabled={enabled}
                hasExplicit={profileIds.length > 0}
                defaultProfiles={defaultProfiles}
              />
              <ProfileToggleList
                profiles={profiles}
                selectedIds={profileIds}
                onChange={setProfileIds}
                disabled={!enabled}
                ariaLabelledBy={PROFILES_LABEL_ID}
              />
            </Field>

            {update.isError && (
              <ApiErrorAlert
                error={update.error}
                fallback="Failed to update DNS filter settings"
              />
            )}
          </CardContent>
          <FormActions
            secondaryLabel="Cancel"
            secondaryProps={{
              onClick: cancelEdit,
              disabled: update.isPending,
            }}
            primaryLabel={update.isPending ? "Saving…" : "Save"}
            primaryProps={{
              onClick: handleSave,
              disabled: update.isPending,
            }}
          />
        </>
      ) : (
        <CardContent>
          <Text
            as="dl"
            size="sm"
            className="grid grid-cols-1 gap-x-8 gap-y-3 sm:grid-cols-2"
          >
            <div>
              <Text
                as="dt"
                size="xs"
                className="uppercase tracking-wide text-ink-3"
              >
                Status
              </Text>
              <dd>{settings.enabled ? "Filtering on" : "Filtering off"}</dd>
            </div>
            <div>
              <Text
                as="dt"
                size="xs"
                className="uppercase tracking-wide text-ink-3"
              >
                Profiles
              </Text>
              <dd>
                {!settings.enabled
                  ? "—"
                  : assignedProfiles.length === 0
                    ? defaultProfiles.length > 0
                      ? `${defaultProfiles.map((p) => p.name).join(", ")} (default)`
                      : "None (no default profile set)"
                    : assignedProfiles.map((p) => p.name).join(", ")}
              </dd>
            </div>
          </Text>
        </CardContent>
      )}
    </Card>
  );
}

interface DefaultProfileHintProps {
  /** Current value of the per-device "DNS filtering" switch in the form. */
  enabled: boolean;
  /** Whether the user has any profile checked in the form. */
  hasExplicit: boolean;
  /** Currently configured global default profiles (resolved from the config's
   *  `default_profile_ids`). */
  defaultProfiles: DnsFilterProfile[];
}

/** Helper text under the Profiles label that explains exactly what the
 *  device will be filtered against given the current form state. Spells
 *  out the default profiles by name so the user isn't left guessing what
 *  "the defaults" mean in this household's setup. */
function DefaultProfileHint({
  enabled,
  hasExplicit,
  defaultProfiles,
}: DefaultProfileHintProps) {
  if (!enabled) {
    return (
      <Text as="p" size="xs" className="text-ink-3">
        Filtering off — DNS queries from this device skip every profile.
      </Text>
    );
  }
  if (hasExplicit) {
    return (
      <Text as="p" size="xs" className="text-ink-3">
        Selected profiles are stacked — a domain blocked in any one of them is
        blocked.
      </Text>
    );
  }
  if (defaultProfiles.length > 0) {
    const noun = defaultProfiles.length === 1 ? "profile" : "profiles";
    return (
      <Text as="p" size="xs" className="text-ink-3">
        No profile selected — this device follows the global default {noun}{" "}
        <Text weight="medium" className="text-ink">
          {defaultProfiles.map((p) => p.name).join(", ")}
        </Text>
        .
      </Text>
    );
  }
  return (
    <Text as="p" size="xs" className="text-ink-3">
      No profile selected and no global default is set — this device's traffic
      isn't filtered.
    </Text>
  );
}

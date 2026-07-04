import { Heading, Text, Toggle } from "@wardnet/web";
import { Button } from "@wardnet/web";
import {
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useUpdateDnsFilterConfig,
} from "@wardnet/web";
import { WizardFooter } from "@/pages/setup/WizardFooter";
import { useWizardNav } from "@/pages/setup/useWizardNav";

/**
 * DNS step — pick the baseline DNS-filter profiles.
 *
 * Shows the built-in profiles (Ad Blocking, Malware & Phishing,
 * Parental Controls) as toggle cards; each toggle adds/removes the
 * profile from the global `default_profile_ids` set applied to devices
 * without an explicit assignment — the same semantics as the DNS
 * Filtering page's per-row toggles. Every write persists immediately
 * via `PUT /api/dns/filter/config` (silently — no toast per toggle),
 * so Continue only advances the wizard.
 */
export default function StepDns() {
  const nav = useWizardNav("dns");
  const { data: profilesData, isLoading } = useDnsFilterProfiles();
  const { data: configData } = useDnsFilterConfig();
  const updateConfig = useUpdateDnsFilterConfig({ silent: true });

  const builtins = (profilesData?.profiles ?? []).filter((p) => p.builtin);
  const defaultIds = configData?.config.default_profile_ids ?? [];

  function toggleDefault(profileId: string, next: boolean) {
    const set = new Set(defaultIds);
    if (next) set.add(profileId);
    else set.delete(profileId);
    updateConfig.mutate({ default_profile_ids: Array.from(set) });
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <Heading level={2} size="3xl" className="text-ink">
          DNS filtering
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          Pick the baseline filtering applied to every device. You can fine-tune
          profiles and per-device overrides later from the DNS page.
        </Text>
      </div>

      {isLoading ? (
        <Text as="p" size="sm" className="text-ink-3">
          Loading profiles…
        </Text>
      ) : (
        <div className="flex flex-col gap-2">
          {builtins.map((profile) => {
            const checked = defaultIds.includes(profile.id);
            return (
              <label
                key={profile.id}
                className={
                  checked
                    ? "flex cursor-pointer items-start justify-between gap-3 rounded-lg border border-accent bg-accent-soft p-3.5"
                    : "flex cursor-pointer items-start justify-between gap-3 rounded-lg border border-line-strong bg-elev p-3.5 hover:bg-sunken"
                }
              >
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <Text
                    as="span"
                    size="sm"
                    weight="medium"
                    className="text-ink"
                  >
                    {profile.name}
                  </Text>
                  {profile.description && (
                    <Text as="span" size="sm" className="text-ink-3">
                      {profile.description}
                    </Text>
                  )}
                </div>
                <Toggle
                  checked={checked}
                  disabled={updateConfig.isPending}
                  onCheckedChange={(next) => toggleDefault(profile.id, next)}
                  data-testid={`setup-dns-profile-${profile.id}`}
                  aria-label={`Enable ${profile.name}`}
                />
              </label>
            );
          })}
        </div>
      )}

      <WizardFooter>
        <Button
          onClick={() => nav.goNext()}
          disabled={nav.isPending}
          data-testid="setup-dns-continue"
          className="w-full"
        >
          {nav.isPending ? "Saving…" : "Continue"}
        </Button>
      </WizardFooter>
    </div>
  );
}

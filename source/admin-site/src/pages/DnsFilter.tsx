import { useMemo, useCallback } from "react";
import { useNavigate } from "react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { ShieldCheck } from "lucide-react";
import { Button } from "@wardnet/web";
import { Card, CardContent } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { DataTable } from "@/components/core/ui/data-table";
import { PageHeader } from "@/components/compound/PageHeader";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { DnsFilterSettingsCard } from "@/components/features/DnsFilterSettingsCard";
import {
  useAllowlist,
  useBlocklists,
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useFilterRules,
  useUpdateDnsFilterConfig,
} from "@wardnet/web";
import type { DnsFilterProfile } from "@wardnet/js";

/** Per-row badges showing the volume of rules attached to a profile.
 *  Zero-valued buckets are omitted so a fresh profile reads as
 *  uncluttered. */
function ProfileCountBadges({ profileId }: { profileId: string }) {
  const { data: blData } = useBlocklists(profileId);
  const { data: alData } = useAllowlist(profileId);
  const { data: frData } = useFilterRules(profileId);

  const blockedCount = (blData?.blocklists ?? []).reduce(
    (sum, b) => sum + b.entry_count,
    0,
  );
  const allowedCount = alData?.entries.length ?? 0;
  const customCount = frData?.rules.length ?? 0;

  return (
    <div className="flex flex-wrap justify-end gap-2">
      {blockedCount > 0 && (
        <Pill variant="ghost">
          {blockedCount.toLocaleString()} blocked domains
        </Pill>
      )}
      {allowedCount > 0 && (
        <Pill variant="ghost">
          {allowedCount.toLocaleString()} allowed domains
        </Pill>
      )}
      {customCount > 0 && (
        <Pill variant="ghost">{customCount.toLocaleString()} custom rules</Pill>
      )}
    </div>
  );
}

/** DNS Filtering page — kill-switch settings card + the single
 *  profile table. Each profile row carries a toggle that controls
 *  whether the profile is part of the default-profile set applied
 *  to devices without an explicit assignment (merges the former
 *  separate "Default profiles" picker into the row chrome). */
export default function DnsFilter() {
  const navigate = useNavigate();
  const { data, isLoading } = useDnsFilterProfiles();
  const { data: configData } = useDnsFilterConfig();
  const updateConfig = useUpdateDnsFilterConfig();

  const profiles = data?.profiles ?? [];
  const hasProfiles = profiles.length > 0;
  const defaultIds = useMemo(
    () => configData?.config.default_profile_ids ?? [],
    [configData?.config.default_profile_ids],
  );
  const filteringEnabled = configData?.config.enabled ?? false;

  const toggleDefault = useCallback(
    (profileId: string, next: boolean) => {
      const set = new Set(defaultIds);
      if (next) set.add(profileId);
      else set.delete(profileId);
      updateConfig.mutate({ default_profile_ids: Array.from(set) });
    },
    [defaultIds, updateConfig],
  );

  function open(profile: DnsFilterProfile) {
    void navigate(`/dns/filter/profiles/${profile.id}`);
  }

  function addProfile() {
    void navigate("/dns/filter/profiles/new");
  }

  const columns = useMemo<ColumnDef<DnsFilterProfile>[]>(
    () => [
      {
        id: "profile",
        header: "Profile",
        cell: ({ row }) => (
          <div className="flex min-w-0 items-center gap-3">
            <div className="grid size-10 shrink-0 place-items-center rounded-lg bg-sunken">
              <ShieldCheck className="size-5 text-ink-3" aria-hidden />
            </div>
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              {/* min-w-0 lets the inner row shrink; the name truncates
                  with ellipsis when the column is narrower than the
                  label. The Builtin pill is `shrink-0` so it stays
                  intact even when the name needs to be clipped. */}
              <div className="flex min-w-0 items-center gap-2">
                <span
                  className="truncate font-medium"
                  title={row.original.name}
                >
                  {row.original.name}
                </span>
                {row.original.builtin && (
                  <Pill variant="ghost" className="shrink-0">
                    Builtin
                  </Pill>
                )}
              </div>
              {row.original.description && (
                <span className="truncate text-xs text-ink-3">
                  {row.original.description}
                </span>
              )}
            </div>
          </div>
        ),
      },
      {
        id: "counts",
        header: "",
        // Hidden on mobile — the pills push the table wider than
        // the viewport and force horizontal scroll. They reappear
        // at md+ where the row has room.
        meta: { className: "hidden text-right md:table-cell" },
        cell: ({ row }) => <ProfileCountBadges profileId={row.original.id} />,
      },
      {
        id: "default",
        header: "Default",
        meta: { className: "w-24 text-right" },
        cell: ({ row }) => {
          const profileId = row.original.id;
          const checked = defaultIds.includes(profileId);
          return (
            // Toggle is not a row navigation — stop propagation so
            // clicking the switch doesn't open the profile detail.
            <div
              className="flex justify-end"
              onClick={(e) => e.stopPropagation()}
            >
              <Toggle
                aria-label={`Use ${row.original.name} as a default profile`}
                checked={checked}
                disabled={!filteringEnabled || updateConfig.isPending}
                onCheckedChange={(next) => toggleDefault(profileId, next)}
              />
            </div>
          );
        },
      },
    ],
    [defaultIds, filteringEnabled, updateConfig.isPending, toggleDefault],
  );

  return (
    <div className="col gap-20">
      <PageHeader
        title="DNS Filtering"
        description="Block ads, trackers, and unwanted domains across every device on the network. Profiles bundle blocklists, allowlists, and custom rules."
        actions={
          hasProfiles ? (
            <Button onClick={addProfile}>Add profile</Button>
          ) : undefined
        }
      />

      <DnsFilterSettingsCard />

      {isLoading && (
        <Card>
          <CardContent className="py-10 text-center text-ink-3">
            Loading profiles…
          </CardContent>
        </Card>
      )}

      {!isLoading && !hasProfiles && (
        <EmptyStatePlaceholder
          message="No DNS filter profiles"
          hint="Profiles bundle blocklists, allowlists, and custom rules together. Assign them to devices on the device detail page."
          actionLabel="Add profile"
          onAction={addProfile}
        />
      )}

      {!isLoading && hasProfiles && (
        <DataTable columns={columns} data={profiles} onRowClick={open} />
      )}
    </div>
  );
}

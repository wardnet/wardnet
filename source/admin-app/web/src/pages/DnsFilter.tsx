import { useNavigate } from "react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { Button } from "@wardnet/forge-web/button";
import { Card, CardContent } from "@wardnet/forge-web/card";
import { DataTable } from "@/components/core/ui/data-table";
import { Pill } from "@wardnet/forge-web/pill";
import { PageHeader } from "@/components/compound/PageHeader";
import { EmptyStatePlaceholder } from "@/components/compound/EmptyStatePlaceholder";
import { useDnsFilterProfiles } from "@/hooks/useDnsFilter";
import type { DnsFilterProfile } from "@wardnet/js";

const columns: ColumnDef<DnsFilterProfile>[] = [
  {
    accessorKey: "name",
    header: "Name",
    cell: ({ row }) => (
      <div className="flex items-center gap-2">
        <span className="font-medium">{row.original.name}</span>
        {row.original.builtin && <Pill variant="ghost">Builtin</Pill>}
      </div>
    ),
  },
];

/** DNS Filter profiles list page (admin only).
 *  Composes `PageHeader` + Forge `Card` (loading), `EmptyStatePlaceholder`
 *  (empty), and the core `DataTable` (populated) inside a `col gap-20`
 *  page wrapper that matches the studio mock (`forge/docs/screens.jsx`
 *  §06). The mock's `.cat` rows live on the profile-detail page; this
 *  list view stays a DataTable per the existing public-API contract.
 *  Public API unchanged — default-exported, no props (consumed via
 *  `<Route element={<DnsFilter />} />` in `App.tsx`). */
export default function DnsFilter() {
  const navigate = useNavigate();
  const { data, isLoading } = useDnsFilterProfiles();
  const profiles = data?.profiles ?? [];
  const hasProfiles = profiles.length > 0;

  function open(profile: DnsFilterProfile) {
    void navigate(`/dns/filter/profiles/${profile.id}`);
  }

  return (
    <div className="col gap-20">
      <PageHeader
        title="DNS Filtering"
        actions={
          hasProfiles ? (
            <Button onClick={() => void navigate("/dns/filter/profiles/new")}>Add profile</Button>
          ) : undefined
        }
      />

      {isLoading && (
        <Card>
          <CardContent className="py-10 text-center text-ink-3">Loading profiles…</CardContent>
        </Card>
      )}

      {!isLoading && !hasProfiles && (
        <EmptyStatePlaceholder
          message="No DNS filter profiles"
          hint="Profiles bundle blocklists, allowlists, and custom rules together. Assign them to devices on the device detail page."
          actionLabel="Add profile"
          onAction={() => void navigate("/dns/filter/profiles/new")}
        />
      )}

      {!isLoading && hasProfiles && (
        <DataTable columns={columns} data={profiles} onRowClick={open} />
      )}
    </div>
  );
}

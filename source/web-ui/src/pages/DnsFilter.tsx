import { useNavigate } from "react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { Button } from "@wardnet/forge/button";
import { Card, CardContent } from "@/components/core/ui/card";
import { DataTable } from "@/components/core/ui/data-table";
import { Badge } from "@/components/core/ui/badge";
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
        {row.original.builtin && <Badge variant="outline">Builtin</Badge>}
      </div>
    ),
  },
];

/** DNS Filter profiles list page (admin only). */
export default function DnsFilter() {
  const navigate = useNavigate();
  const { data, isLoading } = useDnsFilterProfiles();
  const profiles = data?.profiles ?? [];

  function open(profile: DnsFilterProfile) {
    void navigate(`/dns/filter/profiles/${profile.id}`);
  }

  return (
    <>
      <PageHeader title="DNS Filtering" />

      {isLoading && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Loading profiles…
          </CardContent>
        </Card>
      )}

      {!isLoading && profiles.length === 0 && (
        <EmptyStatePlaceholder
          message="No DNS filter profiles"
          hint="Profiles bundle blocklists, allowlists, and custom rules together. Assign them to devices on the device detail page."
          actionLabel="Add profile"
          onAction={() => void navigate("/dns/filter/profiles/new")}
        />
      )}

      {!isLoading && profiles.length > 0 && (
        <div className="flex flex-col gap-4">
          <div className="flex justify-end">
            <Button onClick={() => void navigate("/dns/filter/profiles/new")}>Add profile</Button>
          </div>
          <DataTable columns={columns} data={profiles} onRowClick={open} />
        </div>
      )}
    </>
  );
}

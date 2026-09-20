import { useMemo } from "react";
import type { DataTableColumnDef } from "@/components/core/ui/data-table";
import { Text, timeAgo } from "@wardnet/web";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import type { PrivateDnsGrantSummary } from "@wardnet/js";

interface PrivateDnsGrantsTableProps {
  grants: PrivateDnsGrantSummary[];
  /** Resolve a device id to a human name for the Device column. */
  deviceName: (deviceId: string) => string;
  onSendToDevice: (grant: PrivateDnsGrantSummary) => void;
  onRevoke: (grant: PrivateDnsGrantSummary) => void;
}

function buildColumns(
  deviceName: (deviceId: string) => string,
): DataTableColumnDef<PrivateDnsGrantSummary>[] {
  return [
    {
      id: "device",
      header: "Device",
      cell: ({ row }) => (
        <Text as="span" weight="medium">
          {deviceName(row.original.device_id)}
        </Text>
      ),
    },
    {
      id: "hostname",
      header: "Hostname",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <Text as="span" size="xs" className="font-mono">
          {row.original.hostname}
        </Text>
      ),
    },
    {
      id: "granted",
      header: "Granted",
      cell: ({ row }) => (
        <Text as="span" size="sm" className="text-ink-3">
          {timeAgo(row.original.created_at)}
        </Text>
      ),
    },
  ];
}

/** Per-device Private DNS grants — resend the setup push or revoke, per row.
 *  Presentational: the page owns the hooks and passes callbacks. */
export function PrivateDnsGrantsTable({
  grants,
  deviceName,
  onSendToDevice,
  onRevoke,
}: PrivateDnsGrantsTableProps) {
  const columns = useMemo(() => buildColumns(deviceName), [deviceName]);

  if (grants.length === 0) {
    return (
      <Text as="p" size="sm" className="py-8 text-center text-ink-3">
        No devices have Private DNS access yet.
      </Text>
    );
  }

  return (
    <DataTable
      columns={columns}
      data={grants}
      rowActionsTestId="private-dns-grant-menu"
      rowActions={(grant) => (
        <>
          <RowAction
            onSelect={() => onSendToDevice(grant)}
            testId="private-dns-grant-send"
          >
            Send to device
          </RowAction>
          <RowAction
            onSelect={() => onRevoke(grant)}
            destructive
            testId="private-dns-grant-revoke"
          >
            Revoke
          </RowAction>
        </>
      )}
    />
  );
}

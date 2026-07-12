import { useMemo } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Text } from "@wardnet/web";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import { StatusBadge } from "@/components/compound/StatusBadge";
import type { InboundWgPeerSummary } from "@wardnet/js";

function buildColumns(): ColumnDef<InboundWgPeerSummary>[] {
  return [
    {
      id: "name",
      header: "Device",
      cell: ({ row }) => (
        <Text as="span" weight="medium">
          {row.original.name}
        </Text>
      ),
    },
    {
      id: "allowed_ip",
      header: "Tunnel IP",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <Text as="span" size="xs" className="font-mono">
          {row.original.allowed_ip}
        </Text>
      ),
    },
    {
      id: "status",
      header: "Status",
      cell: ({ row }) => (
        <StatusBadge tone={row.original.enabled ? "success" : "neutral"}>
          {row.original.enabled ? "Active" : "Paused"}
        </StatusBadge>
      ),
    },
  ];
}

interface InboundWgPeersTableProps {
  peers: InboundWgPeerSummary[];
  onToggleEnabled: (peer: InboundWgPeerSummary) => void;
  onRevoke: (peer: InboundWgPeerSummary) => void;
}

/** Peers list for the VPN page's server card — toggle (pause/resume,
 *  keeps the credential) and revoke (delete it) live per row. */
export function InboundWgPeersTable({
  peers,
  onToggleEnabled,
  onRevoke,
}: InboundWgPeersTableProps) {
  const columns = useMemo(() => buildColumns(), []);

  if (peers.length === 0) {
    return (
      <Text as="p" size="sm" className="py-8 text-center text-ink-3">
        No remote-access peers yet.
      </Text>
    );
  }

  return (
    <DataTable
      columns={columns}
      data={peers}
      rowActionsTestId="inbound-wg-peer-menu"
      rowActions={(peer) => (
        <>
          <RowAction
            onSelect={() => onToggleEnabled(peer)}
            testId="inbound-wg-peer-toggle"
          >
            {peer.enabled ? "Pause" : "Resume"}
          </RowAction>
          <RowAction
            onSelect={() => onRevoke(peer)}
            destructive
            testId="inbound-wg-peer-revoke"
          >
            Revoke
          </RowAction>
        </>
      )}
    />
  );
}

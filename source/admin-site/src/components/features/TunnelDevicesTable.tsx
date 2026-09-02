import { useMemo } from "react";
import { useNavigate } from "react-router";
import type { DataTableColumnDef } from "@/components/core/ui/data-table";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  deviceDisplayName,
  sortByLabel,
  Text,
} from "@wardnet/web";
import { DataTable } from "@/components/core/ui/data-table";
import { DeviceIcon } from "@wardnet/web";
import { HostCell } from "@/components/compound/HostCell";
import type { Device } from "@wardnet/js";

interface Props {
  /** Devices routed through the tunnel, from the owning page's
   *  `useTunnelDevices(tunnelId)`. */
  devices: Device[] | undefined;
  isLoading: boolean;
  isError: boolean;
}

function buildColumns(): DataTableColumnDef<Device>[] {
  return [
    {
      accessorKey: "name",
      header: "Device",
      cell: ({ row }) => {
        const device = row.original;
        const primary = deviceDisplayName(device);
        const secondary = primary === device.mac ? null : device.mac;
        return (
          <HostCell
            primary={primary}
            secondary={secondary}
            icon={<DeviceIcon type={device.device_type} />}
          />
        );
      },
    },
    {
      accessorKey: "last_ip",
      header: "IP",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => (
        <span className="text-ink-3">{row.original.last_ip}</span>
      ),
    },
  ];
}

/** Devices routed through a tunnel. Pure presentation — the owning page
 *  wires the query hook and passes data in. */
export function TunnelDevicesTable({
  devices: rawDevices,
  isLoading,
  isError,
}: Props) {
  const navigate = useNavigate();
  const devices = useMemo(
    () => sortByLabel(rawDevices ?? [], deviceDisplayName),
    [rawDevices],
  );
  const columns = useMemo(() => buildColumns(), []);

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          Devices using this tunnel{" "}
          <Text as="span" size="sm" weight="normal" className="text-ink-3">
            ({devices.length})
          </Text>
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isError ? (
          <Text as="p" size="sm" className="text-ink-3">
            Failed to load devices for this tunnel.
          </Text>
        ) : isLoading ? (
          <Text as="p" size="sm" className="text-ink-3">
            Loading…
          </Text>
        ) : (
          <DataTable
            columns={columns}
            data={devices}
            onRowClick={(device) => void navigate(`/devices/${device.id}`)}
            emptyMessage="No devices are currently routed through this tunnel."
          />
        )}
      </CardContent>
    </Card>
  );
}

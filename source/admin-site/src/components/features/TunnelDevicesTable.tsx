import { useMemo } from "react";
import { useNavigate } from "react-router";
import type { ColumnDef } from "@tanstack/react-table";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { DataTable } from "@/components/core/ui/data-table";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { HostCell } from "@/components/compound/HostCell";
import { useTunnelDevices } from "@wardnet/web";
import type { Device } from "@wardnet/js";

interface Props {
  tunnelId: string;
}

function buildColumns(): ColumnDef<Device>[] {
  return [
    {
      accessorKey: "name",
      header: "Device",
      cell: ({ row }) => {
        const device = row.original;
        const primary = device.name ?? device.hostname ?? device.mac;
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

export function TunnelDevicesTable({ tunnelId }: Props) {
  const navigate = useNavigate();
  const { data, isLoading, isError } = useTunnelDevices(tunnelId);
  const devices = data?.devices ?? [];
  const columns = useMemo(() => buildColumns(), []);

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          Devices using this tunnel{" "}
          <span className="text-sm font-normal text-ink-3">
            ({devices.length})
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isError ? (
          <p className="text-sm text-ink-3">
            Failed to load devices for this tunnel.
          </p>
        ) : isLoading ? (
          <p className="text-sm text-ink-3">Loading…</p>
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

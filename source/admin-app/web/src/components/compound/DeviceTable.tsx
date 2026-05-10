import { useMemo } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { Pill } from "@wardnet/forge-web/pill";
import { DataTable } from "@/components/core/ui/data-table";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { StatusBadge } from "@/components/compound/StatusBadge";
import { useDeviceFilterSettingsList, useDnsFilterProfiles } from "@/hooks/useDnsFilter";
import { deviceTypeLabel } from "@/lib/device";
import { timeAgo } from "@/lib/utils";
import type { Device, DeviceDnsFilterSettings, DnsFilterProfile, DhcpStatus } from "@wardnet/js";

function buildColumns(
  settingsByDevice: Map<string, DeviceDnsFilterSettings>,
  profilesById: Map<string, DnsFilterProfile>,
): ColumnDef<Device>[] {
  return [
    {
      accessorKey: "name",
      header: "Device",
      cell: ({ row }) => {
        const device = row.original;
        return (
          <div className="flex items-center gap-3">
            <DeviceIcon type={device.device_type} />
            <div className="flex flex-col">
              <span className="font-medium">{device.name ?? device.hostname ?? device.mac}</span>
              {(device.name || device.hostname) && (
                <span className="text-xs text-ink-3">{device.mac}</span>
              )}
            </div>
          </div>
        );
      },
    },
    {
      accessorKey: "last_ip",
      header: "IP",
      meta: { className: "hidden md:table-cell" },
      cell: ({ row }) => <span className="text-ink-3">{row.original.last_ip}</span>,
    },
    {
      accessorKey: "device_type",
      header: "Type",
      meta: { className: "hidden sm:table-cell" },
      cell: ({ row }) => <Pill variant="ghost">{deviceTypeLabel(row.original.device_type)}</Pill>,
    },
    {
      accessorKey: "manufacturer",
      header: "Manufacturer",
      meta: { className: "hidden lg:table-cell" },
      cell: ({ row }) => <span className="text-ink-3">{row.original.manufacturer ?? "—"}</span>,
    },
    {
      accessorKey: "dhcp_status",
      header: "DHCP",
      meta: { className: "hidden xl:table-cell" },
      cell: ({ row }) => {
        const status: DhcpStatus = row.original.dhcp_status;
        switch (status) {
          case "lease":
            return <StatusBadge tone="success">Lease</StatusBadge>;
          case "reservation":
            return <StatusBadge tone="success">Reserved</StatusBadge>;
          case "external":
            return <StatusBadge tone="neutral">External</StatusBadge>;
        }
      },
    },
    {
      id: "dns_filter",
      header: "DNS Filter",
      meta: { className: "hidden lg:table-cell" },
      cell: ({ row }) => {
        const settings = settingsByDevice.get(row.original.id);
        // No explicit row in the per-device settings table — device follows
        // the default profile (the daemon treats absence as `enabled=true`,
        // empty profile_ids).
        if (!settings) {
          return <StatusBadge tone="neutral">(default)</StatusBadge>;
        }
        if (!settings.enabled) {
          return <StatusBadge tone="neutral">Filtering off</StatusBadge>;
        }
        if (settings.profile_ids.length === 0) {
          return <StatusBadge tone="neutral">(default)</StatusBadge>;
        }
        const names = settings.profile_ids
          .map((id) => profilesById.get(id)?.name)
          .filter((n): n is string => !!n);
        return <StatusBadge tone="success">{names.join(", ") || "—"}</StatusBadge>;
      },
    },
    {
      accessorKey: "last_seen",
      header: "Last seen",
      meta: { className: "text-right" },
      cell: ({ row }) => <span className="text-ink-3">{timeAgo(row.original.last_seen)}</span>,
    },
  ];
}

interface DeviceTableProps {
  devices: Device[];
  onDeviceClick: (deviceId: string) => void;
}

/** Table listing network devices. Receives pre-filtered, pre-sorted data. */
export function DeviceTable({ devices, onDeviceClick }: DeviceTableProps) {
  const { data: settingsData } = useDeviceFilterSettingsList();
  const { data: profilesData } = useDnsFilterProfiles();

  const settingsByDevice = useMemo(() => {
    const map = new Map<string, DeviceDnsFilterSettings>();
    for (const s of settingsData?.devices ?? []) map.set(s.device_id, s);
    return map;
  }, [settingsData]);

  const profilesById = useMemo(() => {
    const map = new Map<string, DnsFilterProfile>();
    for (const p of profilesData?.profiles ?? []) map.set(p.id, p);
    return map;
  }, [profilesData]);

  const columns = useMemo(
    () => buildColumns(settingsByDevice, profilesById),
    [settingsByDevice, profilesById],
  );

  return (
    <DataTable columns={columns} data={devices} onRowClick={(device) => onDeviceClick(device.id)} />
  );
}

import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/core/ui/table";
import { DeviceIcon } from "@/components/compound/DeviceIcon";
import { HostCell } from "@/components/compound/HostCell";
import { useTunnelDevices } from "@/hooks/useTunnels";

interface Props {
  tunnelId: string;
}

export function TunnelDevicesTable({ tunnelId }: Props) {
  const { data, isLoading, isError } = useTunnelDevices(tunnelId);
  const devices = data?.devices ?? [];

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          Devices using this tunnel{" "}
          <span className="text-sm font-normal text-ink-3">({devices.length})</span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        {isError ? (
          <p className="text-sm text-ink-3">Failed to load devices for this tunnel.</p>
        ) : isLoading ? (
          <p className="text-sm text-ink-3">Loading…</p>
        ) : devices.length === 0 ? (
          <p className="text-sm text-ink-3">No devices are currently routed through this tunnel.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Device</TableHead>
                <TableHead className="hidden md:table-cell">IP</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {devices.map((d) => {
                const primary = d.name ?? d.hostname ?? d.mac;
                const secondary = primary === d.mac ? null : d.mac;
                return (
                  <TableRow key={d.id}>
                    <TableCell>
                      <HostCell
                        primary={primary}
                        secondary={secondary}
                        icon={<DeviceIcon type={d.device_type} />}
                      />
                    </TableCell>
                    <TableCell className="hidden font-mono text-xs text-ink-3 md:table-cell">
                      {d.last_ip}
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { timeAgo } from "@/lib/utils";
import type { Device } from "@wardnet/js";

interface DeviceIdentityCardProps {
  device: Device;
}

interface RowProps {
  label: string;
  children: React.ReactNode;
}

function Row({ label, children }: RowProps) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs uppercase tracking-wide text-muted-foreground">{label}</span>
      <span className="text-sm">{children}</span>
    </div>
  );
}

/** Read-only identity card: MAC, hostname, manufacturer, first/last seen. */
export function DeviceIdentityCard({ device }: DeviceIdentityCardProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Identity</CardTitle>
      </CardHeader>
      <CardContent className="grid grid-cols-2 gap-x-6 gap-y-4 md:grid-cols-3">
        <Row label="MAC">
          <span className="font-mono text-xs">{device.mac}</span>
        </Row>
        <Row label="Hostname">{device.hostname ?? "—"}</Row>
        <Row label="Manufacturer">{device.manufacturer ?? "—"}</Row>
        <Row label="First seen">{timeAgo(device.first_seen)}</Row>
        <Row label="Last seen">{timeAgo(device.last_seen)}</Row>
      </CardContent>
    </Card>
  );
}

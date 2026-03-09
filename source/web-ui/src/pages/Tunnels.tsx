import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from "@/components/core/ui/sheet";
import { PageHeader } from "@/components/compound/PageHeader";
import { useTunnels, useCreateTunnel, useDeleteTunnel } from "@/hooks/useTunnels";
import { formatBytes, timeAgo } from "@/lib/utils";
import type { Tunnel, TunnelStatus } from "@wardnet/js";

function statusColor(status: TunnelStatus) {
  const map = {
    up: "default",
    down: "secondary",
    connecting: "outline",
  } as const;
  return map[status];
}

function statusLabel(status: TunnelStatus): string {
  switch (status) {
    case "up":
      return "Active";
    case "down":
      return "Down";
    case "connecting":
      return "Connecting";
  }
}

function TunnelCard({ tunnel, onDelete }: { tunnel: Tunnel; onDelete: (id: string) => void }) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <div className="flex flex-col gap-1">
          <CardTitle className="text-base">{tunnel.label}</CardTitle>
          <p className="text-xs text-muted-foreground">
            {tunnel.country_code.toUpperCase()}
            {tunnel.provider && ` · ${tunnel.provider}`}
          </p>
        </div>
        <Badge variant={statusColor(tunnel.status)}>{statusLabel(tunnel.status)}</Badge>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-y-2 text-sm">
          <div>
            <span className="text-muted-foreground">Interface</span>
            <p className="font-mono text-xs">{tunnel.interface_name}</p>
          </div>
          <div>
            <span className="text-muted-foreground">Endpoint</span>
            <p className="font-mono text-xs">{tunnel.endpoint}</p>
          </div>
          <div>
            <span className="text-muted-foreground">Traffic</span>
            <p className="text-xs">
              {formatBytes(tunnel.bytes_tx)} up / {formatBytes(tunnel.bytes_rx)} down
            </p>
          </div>
          <div>
            <span className="text-muted-foreground">Last handshake</span>
            <p className="text-xs">
              {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "—"}
            </p>
          </div>
        </div>
        <div className="mt-4 flex justify-end">
          <Button variant="destructive" size="sm" onClick={() => onDelete(tunnel.id)}>
            Delete
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function CreateTunnelSheet() {
  const [open, setOpen] = useState(false);
  const [label, setLabel] = useState("");
  const [countryCode, setCountryCode] = useState("");
  const [provider, setProvider] = useState("");
  const [config, setConfig] = useState("");
  const createTunnel = useCreateTunnel();

  function reset() {
    setLabel("");
    setCountryCode("");
    setProvider("");
    setConfig("");
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    await createTunnel.mutateAsync({
      label,
      country_code: countryCode,
      provider: provider || undefined,
      config,
    });
    reset();
    setOpen(false);
  }

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger asChild>
        <Button>Add Tunnel</Button>
      </SheetTrigger>
      <SheetContent className="w-full sm:max-w-md">
        <SheetTitle>Add WireGuard Tunnel</SheetTitle>
        <form onSubmit={handleSubmit} className="mt-6 flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <Label htmlFor="tunnel-label">Label</Label>
            <Input
              id="tunnel-label"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="US West"
              required
            />
          </div>
          <div className="flex gap-3">
            <div className="flex flex-1 flex-col gap-2">
              <Label htmlFor="tunnel-country">Country Code</Label>
              <Input
                id="tunnel-country"
                value={countryCode}
                onChange={(e) => setCountryCode(e.target.value)}
                placeholder="US"
                maxLength={2}
                required
              />
            </div>
            <div className="flex flex-1 flex-col gap-2">
              <Label htmlFor="tunnel-provider">Provider</Label>
              <Input
                id="tunnel-provider"
                value={provider}
                onChange={(e) => setProvider(e.target.value)}
                placeholder="Mullvad"
              />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <Label htmlFor="tunnel-config">WireGuard Config</Label>
            <textarea
              id="tunnel-config"
              value={config}
              onChange={(e) => setConfig(e.target.value)}
              placeholder="Paste your .conf file contents here..."
              required
              rows={10}
              className="rounded-lg border border-input bg-background px-3 py-2 font-mono text-sm focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
            />
          </div>
          {createTunnel.isError && (
            <p className="text-sm text-destructive">
              {createTunnel.error instanceof Error
                ? createTunnel.error.message
                : "Failed to create tunnel"}
            </p>
          )}
          <Button type="submit" disabled={createTunnel.isPending} className="w-full">
            {createTunnel.isPending ? "Creating..." : "Create Tunnel"}
          </Button>
        </form>
      </SheetContent>
    </Sheet>
  );
}

/** Tunnels page for managing WireGuard VPN tunnels (admin only). */
export default function Tunnels() {
  const { data, isLoading, isError } = useTunnels();
  const deleteTunnel = useDeleteTunnel();
  const tunnels = data?.tunnels ?? [];

  return (
    <>
      <PageHeader title="Tunnels" actions={<CreateTunnelSheet />} />

      {isLoading && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Loading tunnels...
          </CardContent>
        </Card>
      )}

      {isError && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Failed to load tunnels. Make sure the daemon is running.
          </CardContent>
        </Card>
      )}

      {!isLoading && !isError && tunnels.length === 0 && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            No tunnels configured. Add a WireGuard tunnel to get started.
          </CardContent>
        </Card>
      )}

      {tunnels.length > 0 && (
        <div className="grid gap-4 md:grid-cols-2">
          {tunnels.map((tunnel) => (
            <TunnelCard
              key={tunnel.id}
              tunnel={tunnel}
              onDelete={(id) => deleteTunnel.mutate(id)}
            />
          ))}
        </div>
      )}
    </>
  );
}

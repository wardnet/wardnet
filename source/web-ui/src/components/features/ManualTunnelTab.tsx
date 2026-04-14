import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Textarea } from "@/components/core/ui/textarea";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useCreateTunnel } from "@/hooks/useTunnels";

interface ManualTunnelTabProps {
  onSuccess: () => void;
}

/** Manual WireGuard config paste form for creating a tunnel. */
export function ManualTunnelTab({ onSuccess }: ManualTunnelTabProps) {
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
    onSuccess();
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-4">
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
        <Textarea
          id="tunnel-config"
          value={config}
          onChange={(e) => setConfig(e.target.value)}
          placeholder="Paste your .conf file contents here..."
          required
          rows={10}
          className="font-mono"
        />
      </div>
      {createTunnel.isError && (
        <ApiErrorAlert error={createTunnel.error} fallback="Failed to create tunnel" />
      )}
      <Button type="submit" disabled={createTunnel.isPending} className="w-full">
        {createTunnel.isPending ? "Creating..." : "Create Tunnel"}
      </Button>
    </form>
  );
}

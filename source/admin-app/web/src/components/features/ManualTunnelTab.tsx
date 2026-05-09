import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import { Textarea } from "@wardnet/forge-web/textarea";
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
      <div className="grid grid-cols-1 gap-x-6 gap-y-4 md:grid-cols-2">
        <Field label="Label" htmlFor="tunnel-label">
          <Input
            id="tunnel-label"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="US West"
            required
          />
        </Field>
        <Field label="Country code" htmlFor="tunnel-country">
          <Input
            id="tunnel-country"
            value={countryCode}
            onChange={(e) => setCountryCode(e.target.value)}
            placeholder="US"
            maxLength={2}
            required
          />
        </Field>
        <Field label="Provider" htmlFor="tunnel-provider" className="md:col-span-2">
          <Input
            id="tunnel-provider"
            value={provider}
            onChange={(e) => setProvider(e.target.value)}
            placeholder="Mullvad"
          />
        </Field>
        <Field label="WireGuard config" htmlFor="tunnel-config" className="md:col-span-2">
          <Textarea
            id="tunnel-config"
            value={config}
            onChange={(e) => setConfig(e.target.value)}
            placeholder="Paste your .conf file contents here…"
            required
            rows={10}
            className="font-mono"
          />
        </Field>
      </div>
      {createTunnel.isError && (
        <ApiErrorAlert error={createTunnel.error} fallback="Failed to create tunnel" />
      )}
      <div className="flex justify-end">
        <Button type="submit" disabled={createTunnel.isPending}>
          {createTunnel.isPending ? "Creating…" : "Create tunnel"}
        </Button>
      </div>
    </form>
  );
}

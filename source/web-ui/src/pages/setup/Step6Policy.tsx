import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { useAdvanceWizard } from "@/hooks/useSetup";
import { useDefaultPolicy, useSetDefaultPolicy } from "@/hooks/useDefaultPolicy";
import { useTunnels } from "@/hooks/useTunnels";

/**
 * Step 6 — pick the global default routing policy.
 *
 * Picker enumerates `["Direct", ...tunnels]` and auto-selects Direct
 * when no tunnels exist. Persists via `PUT /api/system/default-policy`
 * before advancing to the confirmation step.
 */
export default function Step6Policy() {
  const advance = useAdvanceWizard();
  const setDefault = useSetDefaultPolicy();
  const { data: current } = useDefaultPolicy();
  const { data: tunnels } = useTunnels();
  const tunnelList = tunnels?.tunnels ?? [];

  // Default to whatever the daemon already has; fall back to "direct".
  // The user's pick (when set) takes precedence over the persisted value.
  const [override, setOverride] = useState<string | null>(null);
  const policy = override ?? current?.policy ?? "direct";

  async function handleContinue() {
    await setDefault.mutateAsync(policy);
    await advance.mutateAsync({ to_step: "completed" });
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">Default routing</h2>
        <p className="text-sm text-muted-foreground">
          Pick how new devices route by default. You can override per-device later from the Devices
          page.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <Select value={policy} onValueChange={setOverride}>
          <SelectTrigger className="h-12">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="direct">Direct (no VPN)</SelectItem>
            {tunnelList.map((t) => (
              <SelectItem key={t.id} value={t.id}>
                Tunnel — {t.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {tunnelList.length === 0 && (
          <p className="text-xs text-muted-foreground">
            No tunnels configured — defaulting to direct routing. You can change this from Settings
            whenever you add a tunnel.
          </p>
        )}
      </div>

      <Button
        onClick={handleContinue}
        disabled={setDefault.isPending || advance.isPending}
        className="h-12 w-full"
      >
        {setDefault.isPending || advance.isPending ? "Saving…" : "Continue"}
      </Button>
    </div>
  );
}

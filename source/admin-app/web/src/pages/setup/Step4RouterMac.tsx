import { useEffect, useRef, useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { useAdvanceWizard } from "@/hooks/useSetup";
import { useDiscoverGatewayMac } from "@/hooks/useNetwork";

/**
 * Step 4 — discover the upstream router MAC.
 *
 * Auto-fires the ARP probe on first render. On success the operator
 * just confirms; on failure the manual-entry field surfaces and
 * submits back through the same endpoint with `{mac}` set so the
 * daemon persists either path identically.
 */
export default function Step4RouterMac() {
  const advance = useAdvanceWizard();
  const probe = useDiscoverGatewayMac();
  const [manualMac, setManualMac] = useState("");
  const triedRef = useRef(false);

  // Kick off the probe once on mount. A ref guard (rather than a
  // dependency on `probe`) keeps re-renders from re-firing the probe
  // and stomping on a manual entry the operator may have typed.
  useEffect(() => {
    if (triedRef.current) return;
    triedRef.current = true;
    probe.mutate({});
  }, [probe]);

  const probedMac = probe.data?.mac;
  const probedSource = probe.data?.source;
  const probeFailed = probe.isError;

  async function handleManualSubmit(e: React.FormEvent) {
    e.preventDefault();
    await probe.mutateAsync({ mac: manualMac });
  }

  async function handleContinue() {
    await advance.mutateAsync({ to_step: "tunnel" });
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">Router MAC</h2>
        <p className="text-sm text-muted-foreground">
          Wardnet uses the upstream router's MAC address for diagnostics and packet-capture
          filtering.
        </p>
      </div>

      {probe.isPending && !probedMac && (
        <p className="text-sm text-muted-foreground">Probing the gateway via ARP…</p>
      )}

      {probedMac && (
        <div className="rounded-lg border border-border bg-muted/30 p-4 text-sm">
          <p className="font-medium text-foreground">
            {probedSource === "arp" ? "Discovered via ARP" : "Recorded"}
          </p>
          <p className="mt-1 font-mono">{probedMac}</p>
        </div>
      )}

      {!probe.isPending && !probedMac && probeFailed && (
        <form onSubmit={handleManualSubmit} className="flex flex-col gap-2">
          <Label htmlFor="router-mac" className="text-foreground/70">
            ARP probe failed — enter the router MAC manually
          </Label>
          <Input
            id="router-mac"
            value={manualMac}
            onChange={(e) => setManualMac(e.target.value)}
            placeholder="00:11:22:AA:BB:CC"
            className="h-12 font-mono"
          />
          <div className="flex justify-end">
            <Button type="submit" variant="secondary" disabled={!manualMac}>
              Save MAC
            </Button>
          </div>
        </form>
      )}

      <Button
        onClick={handleContinue}
        disabled={advance.isPending}
        className="h-12 w-full"
      >
        {advance.isPending ? "Saving…" : probedMac ? "Continue" : "Skip"}
      </Button>
    </div>
  );
}

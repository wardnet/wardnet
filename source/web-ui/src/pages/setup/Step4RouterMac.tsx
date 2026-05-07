import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { useAdvanceWizard } from "@/hooks/useSetup";

/**
 * Step 4 — discover the upstream router MAC.
 *
 * The intended UX (after the daemon's
 * `POST /api/network/discover-gateway-mac` lands) is a silent ARP probe
 * that auto-advances on success and only surfaces this manual-entry
 * field on failure. Until that endpoint exists this step always shows
 * the manual field so the operator can type the MAC and continue.
 */
export default function Step4RouterMac() {
  const advance = useAdvanceWizard();
  const [mac, setMac] = useState("");

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">Router MAC</h2>
        <p className="text-sm text-muted-foreground">
          Wardnet uses the upstream router's MAC address for diagnostics and packet-capture
          filtering.
        </p>
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="router-mac" className="text-foreground/70">
          Router MAC (e.g. 00:11:22:AA:BB:CC)
        </Label>
        <Input
          id="router-mac"
          value={mac}
          onChange={(e) => setMac(e.target.value)}
          placeholder="00:11:22:AA:BB:CC"
          className="h-12"
        />
        <p className="text-xs text-muted-foreground">
          Auto-discovery via ARP probe lands in a follow-up; for now you can paste the MAC from your
          router's admin panel or skip ahead.
        </p>
      </div>
      <Button
        onClick={() => advance.mutate({ to_step: "tunnel" })}
        disabled={advance.isPending}
        className="h-12 w-full"
      >
        {advance.isPending ? "Saving…" : "Continue"}
      </Button>
    </div>
  );
}

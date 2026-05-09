import { useState } from "react";
import type { WizardMode } from "@wardnet/js";
import { Button } from "@wardnet/forge/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { useAdvanceWizard } from "@/hooks/useSetup";
import { useDhcpSelfProbe } from "@/hooks/useNetwork";
import { ROUTER_INSTRUCTIONS, findRouterInstruction } from "@/lib/router-instructions";

/**
 * Step 3 — DHCP onboarding.
 *
 * Branches on the operator's chosen `mode`:
 *
 * - `primary`: Wardnet runs DHCP. The operator picks their router,
 *   follows the disable-DHCP steps, and clicks "Probe LAN". The
 *   daemon broadcasts a DHCPDISCOVER and reports back which servers
 *   replied; only after a clean (Wardnet-only) probe can the wizard
 *   advance.
 * - `locked_router`: ISP router keeps running DHCP. Operators
 *   manually configure each opted-in device — there's nothing to
 *   probe, so the step jumps straight to Continue.
 *
 * The `wizard_mode` selection is recorded on advance so later steps
 * know which UX variant to render.
 */
export default function Step3DhcpOnboarding({ initialMode }: { initialMode: WizardMode | null }) {
  const advance = useAdvanceWizard();
  const probe = useDhcpSelfProbe();
  const [mode, setMode] = useState<WizardMode>(initialMode ?? "primary");
  const [routerId, setRouterId] = useState<string>(ROUTER_INSTRUCTIONS[0].id);
  const router = findRouterInstruction(routerId);

  const probeResult = probe.data;
  const probeClean =
    mode === "primary" && probeResult?.wardnet_responded && !probeResult.foreign_responded;
  const probeBlocked = mode === "primary" && probeResult?.foreign_responded;

  function handleContinue() {
    advance.mutate({ to_step: "router_mac", wizard_mode: mode });
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">DHCP onboarding</h2>
        <p className="text-sm text-muted-foreground">
          Choose how Wardnet handles DHCP on your LAN.
        </p>
      </div>

      <div className="flex flex-col gap-2">
        <label className="flex cursor-pointer items-start gap-3 rounded-lg border border-border p-3">
          <input
            type="radio"
            name="dhcp-mode"
            value="primary"
            checked={mode === "primary"}
            onChange={() => setMode("primary")}
            className="mt-1"
          />
          <div className="flex flex-col gap-1">
            <span className="font-medium">Primary (recommended)</span>
            <span className="text-sm text-muted-foreground">
              Wardnet runs DHCP. You'll disable DHCP on your existing router using the steps below.
            </span>
          </div>
        </label>
        <label className="flex cursor-pointer items-start gap-3 rounded-lg border border-border p-3">
          <input
            type="radio"
            name="dhcp-mode"
            value="locked_router"
            checked={mode === "locked_router"}
            onChange={() => setMode("locked_router")}
            className="mt-1"
          />
          <div className="flex flex-col gap-1">
            <span className="font-medium">Locked router</span>
            <span className="text-sm text-muted-foreground">
              Your ISP router can't disable DHCP. You'll manually point each opted-in device at
              Wardnet as its gateway and DNS.
            </span>
          </div>
        </label>
      </div>

      {mode === "primary" && (
        <div className="flex flex-col gap-3 rounded-lg border border-border bg-muted/30 p-4">
          <div className="flex flex-col gap-1">
            <span className="text-sm font-medium">Pick your router</span>
            <Select value={routerId} onValueChange={setRouterId}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ROUTER_INSTRUCTIONS.map((r) => (
                  <SelectItem key={r.id} value={r.id}>
                    {r.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          {router && (
            <div className="flex flex-col gap-2">
              <ol className="ml-5 list-decimal space-y-1 text-sm text-muted-foreground">
                {router.steps.map((step, i) => (
                  <li key={i}>{step}</li>
                ))}
              </ol>
              {router.kb_url && (
                <a
                  href={router.kb_url}
                  target="_blank"
                  rel="noreferrer"
                  className="text-xs text-primary hover:underline"
                >
                  {router.name} support page ↗
                </a>
              )}
            </div>
          )}
          <div className="flex items-center justify-end gap-3">
            {probe.isError && (
              <p className="flex-1 text-sm text-destructive">Probe failed — try again.</p>
            )}
            {probeBlocked && (
              <p className="flex-1 text-sm text-destructive">
                A foreign DHCP server responded
                {probeResult?.foreign_server_ip ? ` (${probeResult.foreign_server_ip})` : ""}.
                Re-check the disable-DHCP steps.
              </p>
            )}
            {probeClean && (
              <p className="flex-1 text-sm text-foreground">Only Wardnet responded — good.</p>
            )}
            <Button variant="secondary" onClick={() => probe.mutate()} disabled={probe.isPending}>
              {probe.isPending ? "Probing…" : probeResult ? "Probe again" : "Probe LAN"}
            </Button>
          </div>
        </div>
      )}

      {mode === "locked_router" && (
        <div className="flex flex-col gap-2 rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted-foreground">
          <p>
            Configure each opted-in device's network settings to use Wardnet as its gateway and DNS
            server. Other devices on the LAN keep their normal connectivity.
          </p>
          <p>
            On the next page Wardnet will show a live counter of devices it sees using it as a
            gateway.
          </p>
        </div>
      )}

      <Button
        onClick={handleContinue}
        disabled={advance.isPending || (mode === "primary" && !probeClean)}
        className="h-12 w-full"
      >
        {advance.isPending
          ? "Saving…"
          : mode === "primary" && !probeClean
            ? "Run a clean probe to continue"
            : "Continue"}
      </Button>
    </div>
  );
}

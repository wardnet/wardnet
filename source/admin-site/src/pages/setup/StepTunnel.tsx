import { useState } from "react";
import { Button } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";
import { CreateTunnelInline } from "@/components/features/CreateTunnelInline";
import { useTunnels } from "@wardnet/web";
import { WizardFooter } from "@/pages/setup/WizardFooter";
import { useWizardNav } from "@/pages/setup/useWizardNav";

/**
 * Tunnel step — first VPN tunnel (optional).
 *
 * Inlines `CreateTunnelInline` (the same component the Tunnels page
 * uses) directly inside the auth card so the operator can import a
 * tunnel without leaving the wizard — `SetupGuard` would bounce them
 * straight back here if they tried to navigate to `/tunnels` before
 * finishing setup.
 *
 * The Skip button always advances to the policy step, whose picker
 * defaults to "direct" when no tunnels exist.
 */
export default function StepTunnel() {
  const nav = useWizardNav("tunnel");
  const { data: tunnels } = useTunnels();
  const [adding, setAdding] = useState(false);
  const tunnelCount = tunnels?.tunnels.length ?? 0;
  const hasTunnel = tunnelCount > 0;

  if (adding) {
    return (
      <>
        <CreateTunnelInline onClose={() => setAdding(false)} embedded />
        {/* Keep the docked footer populated while the inline form
            replaces the step body, so the shell's footer never empties. */}
        <WizardFooter>
          <Button
            variant="secondary"
            onClick={() => setAdding(false)}
            className="w-full"
          >
            Back to tunnel overview
          </Button>
        </WizardFooter>
      </>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <Heading level={2} size="3xl" className="text-ink">
          First VPN tunnel
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          Optional — connect a VPN provider so opted-in devices can route
          through it. You can add more from the Tunnels page once setup is
          complete.
        </Text>
      </div>

      {hasTunnel ? (
        <Text
          as="div"
          size="sm"
          className="rounded-md border border-line bg-sunken p-4 text-ink-3"
        >
          {tunnelCount} tunnel{tunnelCount === 1 ? "" : "s"} configured.
          Continue to pick a default routing policy.
        </Text>
      ) : (
        <Text
          as="div"
          size="sm"
          className="rounded-md border border-line bg-sunken p-4 text-ink-3"
        >
          No tunnels yet. Add one now or skip — you can change the default
          routing policy from Settings later.
        </Text>
      )}

      <Button
        variant="outline"
        onClick={() => setAdding(true)}
        className="w-full"
      >
        {hasTunnel ? "Add another tunnel" : "Add tunnel"}
      </Button>

      <WizardFooter>
        <Button
          onClick={() => nav.goNext()}
          disabled={nav.isPending}
          data-testid="setup-tunnel-skip"
          className="w-full"
        >
          {nav.isPending ? "Saving…" : hasTunnel ? "Continue" : "Skip for now"}
        </Button>
      </WizardFooter>
    </div>
  );
}

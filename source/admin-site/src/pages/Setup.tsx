import { useSetupStatus } from "@wardnet/web";
import { Text } from "@wardnet/web";
import StepAdmin from "@/pages/setup/StepAdmin";
import StepNetwork from "@/pages/setup/StepNetwork";
import StepDhcp from "@/pages/setup/StepDhcp";
import StepRouterMac from "@/pages/setup/StepRouterMac";
import StepDns from "@/pages/setup/StepDns";
import StepTunnel from "@/pages/setup/StepTunnel";
import StepPolicy from "@/pages/setup/StepPolicy";
import StepRemoteAccess from "@/pages/setup/StepRemoteAccess";
import StepReview from "@/pages/setup/StepReview";
import StepDone from "@/pages/setup/StepDone";
import { WizardShell } from "@/pages/setup/WizardShell";

/**
 * First-run setup wizard.
 *
 * Dispatches on `wizard_step` from `GET /api/setup/status`. Step 1
 * (admin creation) is unauthenticated; steps 2 onwards run as the
 * auto-logged-in admin and call `POST /api/setup/advance` to record
 * progress (forward, or rewinding via the rail / back link down to
 * the `network` floor).
 *
 * Renders its own full-page shell (not `AuthLayout`): the fixed-shape
 * "guided rail" card keeps the docked footer CTA at the same position
 * on every step — see `WizardShell`.
 */
export default function Setup() {
  const { data, isLoading } = useSetupStatus();

  return (
    <div className="flex min-h-screen items-center justify-center bg-bg px-4 py-10 text-ink">
      <WizardShell current={data?.wizard_step ?? "admin"}>
        {isLoading || !data ? (
          <Text as="p" size="sm" className="text-ink-3">
            Loading…
          </Text>
        ) : (
          <>
            {data.wizard_step === "admin" && <StepAdmin />}
            {data.wizard_step === "network" && <StepNetwork />}
            {data.wizard_step === "dhcp" && (
              <StepDhcp initialMode={data.wizard_mode} />
            )}
            {data.wizard_step === "router_mac" && <StepRouterMac />}
            {data.wizard_step === "dns" && <StepDns />}
            {data.wizard_step === "tunnel" && <StepTunnel />}
            {data.wizard_step === "policy" && <StepPolicy />}
            {data.wizard_step === "remote_access" && <StepRemoteAccess />}
            {data.wizard_step === "review" && <StepReview />}
            {data.wizard_step === "completed" && <StepDone />}
          </>
        )}
      </WizardShell>
    </div>
  );
}

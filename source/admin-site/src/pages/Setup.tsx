import { useSetupStatus } from "@wardnet/web";
import { Text } from "@wardnet/web";
import Step1Admin from "@/pages/setup/Step1Admin";
import Step2Network from "@/pages/setup/Step2Network";
import Step3DhcpOnboarding from "@/pages/setup/Step3DhcpOnboarding";
import Step4RouterMac from "@/pages/setup/Step4RouterMac";
import Step5Tunnel from "@/pages/setup/Step5Tunnel";
import Step6Policy from "@/pages/setup/Step6Policy";
import Step7RemoteAccess from "@/pages/setup/Step7RemoteAccess";
import Step8Confirm from "@/pages/setup/Step8Confirm";
import { WizardStepper } from "@/pages/setup/WizardStepper";

/**
 * First-run setup wizard.
 *
 * Dispatches on `wizard_step` from `GET /api/setup/status`. Step 1
 * (admin creation) is unauthenticated; steps 2 onwards run as the
 * auto-logged-in admin and call `POST /api/setup/advance` to record
 * progress.
 */
export default function Setup() {
  const { data, isLoading } = useSetupStatus();

  // AuthLayout already wraps the routed page in a Forge `<Card>` (slice
  // T5-5b), so this shell is chrome-free — no second card, no shadow,
  // no white wrapper. `min-h-[36rem]` keeps the card a stable size
  // across steps so the surrounding hero (logo + subtitle in
  // AuthLayout) doesn't bounce up and down as content varies in
  // height. Short steps simply leave empty space below their action
  // button rather than letting the card collapse; tall steps (DHCP
  // onboarding, the inline tunnel form) extend naturally past the
  // min-height — downward growth is far less jarring than the card
  // shrinking step-to-step.
  return (
    <div className="min-h-[36rem]">
      {isLoading || !data ? (
        <Text as="p" size="sm" className="text-ink-3">
          Loading…
        </Text>
      ) : (
        <>
          <WizardStepper current={data.wizard_step} />
          {data.wizard_step === "admin" && <Step1Admin />}
          {data.wizard_step === "network" && <Step2Network />}
          {data.wizard_step === "dhcp" && (
            <Step3DhcpOnboarding initialMode={data.wizard_mode} />
          )}
          {data.wizard_step === "router_mac" && <Step4RouterMac />}
          {data.wizard_step === "tunnel" && <Step5Tunnel />}
          {data.wizard_step === "policy" && <Step6Policy />}
          {data.wizard_step === "remote_access" && <Step7RemoteAccess />}
          {data.wizard_step === "completed" && <Step8Confirm />}
        </>
      )}
    </div>
  );
}

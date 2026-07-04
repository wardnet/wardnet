import { useNavigate } from "react-router";
import { CheckIcon } from "lucide-react";
import { Button } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";
import { WizardFooter } from "@/pages/setup/WizardFooter";

/**
 * Done step — wizard finished.
 *
 * Reached after the review step advances `wizard_step` to
 * `"completed"`. The rail renders every step as done and this page lets the
 * operator jump to the dashboard. SetupGuard sees `wizard_step === "completed"`
 * and stops redirecting back to /setup.
 */
export default function StepDone() {
  const navigate = useNavigate();

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <Heading level={2} size="3xl" className="text-ink">
          All set
        </Heading>
        <Text as="p" size="sm" className="mt-1 text-ink-3">
          Wardnet is configured. You can manage devices, tunnels, DHCP, and DNS
          filtering from the dashboard.
        </Text>
      </div>
      <div className="flex size-16 items-center justify-center rounded-full bg-accent-soft text-accent-soft-ink">
        <CheckIcon className="size-7" />
      </div>
      <WizardFooter>
        <Button
          onClick={() => navigate("/")}
          data-testid="setup-go-dashboard"
          className="w-full"
        >
          Go to dashboard
        </Button>
      </WizardFooter>
    </div>
  );
}

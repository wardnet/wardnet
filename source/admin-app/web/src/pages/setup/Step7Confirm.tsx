import { useNavigate } from "react-router";
import { Button } from "@wardnet/forge-web/button";

/**
 * Step 7 — wizard finished.
 *
 * Reached after step 6 advances `wizard_step` to `"completed"`. The
 * stepper renders the "Done" pill and this page lets the operator
 * jump to the dashboard. SetupGuard sees `wizard_step === "completed"`
 * and stops redirecting back to /setup.
 */
export default function Step7Confirm() {
  const navigate = useNavigate();

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="text-lg font-semibold text-foreground">All set</h2>
        <p className="text-sm text-muted-foreground">
          Wardnet is configured. You can manage devices, tunnels, DHCP, and DNS filtering from the
          dashboard.
        </p>
      </div>
      <Button onClick={() => navigate("/")} className="h-12 w-full">
        Go to dashboard
      </Button>
    </div>
  );
}

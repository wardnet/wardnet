import { useTlsStatus } from "@wardnet/wardnet-web";
import { SecureAccessProgress } from "@/components/features/SecureAccessProgress";

/**
 * Persistent dashboard indicator for secure-access provisioning. Surfaces the
 * background certificate issuance the setup wizard kicks off, so an operator who
 * finished the wizard before the cert landed still sees progress (`issuing`) and
 * any `failed` attempt without digging into Settings.
 *
 * Shows only the in-flight / needs-attention phases. `issued` is deliberately
 * suppressed here: a permanent "secure access is live" card would clutter every
 * dashboard load forever, and the steady-state certificate panel (with manual
 * retry) is owned by Settings (C10). The wizard's own step still shows the
 * issued confirmation inline. Renders nothing while idle. Polls only while
 * issuance is in flight (the hook stops once a terminal phase is reached).
 */
export function DashboardSecureAccessBanner() {
  const { data: status } = useTlsStatus();

  if (!status || status.phase === "idle" || status.phase === "issued") return null;

  return <SecureAccessProgress status={status} />;
}

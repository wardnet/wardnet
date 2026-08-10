import { AlertTriangle } from "lucide-react";
import { Banner } from "@wardnet/web";

interface ConnectionBannerProps {
  /** Whether the daemon answered the last status probe; `undefined` while
   *  the shell layout's status query is still unresolved (banner hidden). */
  reachable: boolean | undefined;
}

/**
 * Full-width red banner shown at the top of all pages when the daemon is unreachable.
 * Fed by the same unauthenticated /api/info endpoint as the sidebar connection indicator.
 *
 * Thin wrapper over the Forge `<Banner>` primitive — visual shape lives in
 * the `.banner` recipe (styles.css §05); this component only decides whether
 * to show the strip and what to put inside it. Pure presentation — the shell
 * layout wires the status hook and passes the state in.
 */
export function ConnectionBanner({ reachable }: ConnectionBannerProps) {
  if (reachable !== false) return null;

  return (
    <Banner tone="down" icon={<AlertTriangle />} role="alert">
      Unable to connect to the Wardnet daemon. Make sure it is running.
    </Banner>
  );
}

import type { AccessRequestStatus } from "@wardnet/js";
import { Pill } from "@wardnet/ui";

interface AccessRequestStatusPillProps {
  status: AccessRequestStatus;
}

/** Shared status badge for device access requests (user PWA + admin inbox). */
export function AccessRequestStatusPill({
  status,
}: AccessRequestStatusPillProps) {
  if (status === "approved") return <Pill variant="ok">Approved</Pill>;
  if (status === "rejected") return <Pill variant="down">Declined</Pill>;
  return <Pill variant="info">Pending</Pill>;
}

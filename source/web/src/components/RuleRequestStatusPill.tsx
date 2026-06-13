import type { RuleRequestStatus } from "@wardnet/js";
import { Pill } from "../primitives/pill";

interface RuleRequestStatusPillProps {
  status: RuleRequestStatus;
}

/** Shared status badge for device rule requests (user PWA + admin inbox). */
export function RuleRequestStatusPill({ status }: RuleRequestStatusPillProps) {
  if (status === "approved") return <Pill variant="ok">Approved</Pill>;
  if (status === "rejected") return <Pill variant="down">Rejected</Pill>;
  return <Pill variant="info">Pending</Pill>;
}

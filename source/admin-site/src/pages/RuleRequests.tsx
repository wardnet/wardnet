import { useState } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import {
  ApiErrorAlert,
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
  FormActions,
  RuleRequestStatusPill,
  SegmentedTabs,
  deviceDisplayName,
  useDecideRuleRequest,
  useDevices,
  useRuleRequests,
} from "@wardnet/web";
import type { Device, DeviceRuleRequest, RuleRequestStatus } from "@wardnet/js";

const ALL = "all";

function deviceLabel(device: Device | undefined, deviceId: string): string {
  if (!device) return `Unknown device (${deviceId.slice(0, 8)})`;
  return deviceDisplayName(device);
}

function RequestRow({
  req,
  deviceName,
}: {
  req: DeviceRuleRequest;
  deviceName: string;
}) {
  const decide = useDecideRuleRequest();
  const pending = req.status === "pending";

  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {req.kind === "block" ? "Block request" : "Allow request"}
        </CardTitle>
        <CardAction>
          <RuleRequestStatusPill status={req.status} />
        </CardAction>
      </CardHeader>

      <CardContent className="flex flex-col gap-1">
        <span className="font-mono text-sm text-ink">{req.domain}</span>
        <span className="text-xs text-ink-3">
          {deviceName} · {new Date(req.created_at).toLocaleString()}
        </span>

        {req.reason && (
          <p className="mt-2 rounded-lg bg-sunken px-3 py-2 text-sm text-ink-2">
            “{req.reason}”
          </p>
        )}

        {decide.isError && (
          <ApiErrorAlert
            error={decide.error}
            fallback="Failed to update request"
          />
        )}
      </CardContent>

      {pending && (
        <FormActions
          secondaryLabel="Reject"
          onSecondary={() => decide.mutate({ id: req.id, status: "rejected" })}
          primaryLabel="Approve"
          onPrimary={() => decide.mutate({ id: req.id, status: "approved" })}
          disabled={decide.isPending}
        />
      )}
    </Card>
  );
}

/**
 * Admin inbox for household rule requests. Approving/rejecting records the
 * decision; applying the actual DNS rule is still done via the DNS filter UI.
 */
export default function RuleRequests() {
  const [filter, setFilter] = useState<RuleRequestStatus | undefined>(
    "pending",
  );
  // Fetch every request once and filter/count client-side (same pattern as the
  // DHCP table) so the tabs can show per-status counters.
  const { data, isLoading, isError, error } = useRuleRequests();
  const { data: devicesData } = useDevices();

  const deviceById = new Map(
    (devicesData?.devices ?? []).map((d) => [d.id, d]),
  );

  const all = data ?? [];
  const countOf = (s: RuleRequestStatus) =>
    all.filter((r) => r.status === s).length;
  const tabs = [
    { id: "pending", label: "Pending", count: countOf("pending") },
    { id: "approved", label: "Approved", count: countOf("approved") },
    { id: "rejected", label: "Rejected", count: countOf("rejected") },
    { id: ALL, label: "All", count: all.length },
  ];
  const visible = filter ? all.filter((r) => r.status === filter) : all;

  return (
    <>
      <PageHeader
        title="Rule requests"
        description="Block / allow requests from household devices. Approving records the decision — apply the rule in DNS Filtering."
      />

      <div className="flex flex-col gap-4">
        <SegmentedTabs
          tabs={tabs}
          activeId={filter ?? ALL}
          onChange={(id) =>
            setFilter(id === ALL ? undefined : (id as RuleRequestStatus))
          }
        />

        {isLoading && <p className="text-sm text-ink-3">Loading…</p>}
        {isError && (
          <ApiErrorAlert
            error={error}
            fallback="Failed to load rule requests"
          />
        )}
        {!isLoading && visible.length === 0 && (
          <p className="text-sm text-ink-3">No requests.</p>
        )}

        {visible.map((req) => (
          <RequestRow
            key={req.id}
            req={req}
            deviceName={deviceLabel(
              deviceById.get(req.device_id),
              req.device_id,
            )}
          />
        ))}
      </div>
    </>
  );
}

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
  Text,
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
        <Text size="sm" className="font-mono text-ink">
          {req.domain}
        </Text>
        <Text size="xs" className="text-ink-3">
          {deviceName} · {new Date(req.created_at).toLocaleString()}
        </Text>

        {req.reason && (
          <Text
            as="p"
            size="sm"
            className="mt-2 rounded-lg bg-sunken px-3 py-2 text-ink-2"
          >
            “{req.reason}”
          </Text>
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
          secondaryProps={{
            onClick: () => decide.mutate({ id: req.id, status: "rejected" }),
            disabled: decide.isPending,
          }}
          primaryLabel="Approve"
          primaryProps={{
            onClick: () => decide.mutate({ id: req.id, status: "approved" }),
            disabled: decide.isPending,
          }}
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

        {isLoading && (
          <Text as="p" size="sm" className="text-ink-3">
            Loading…
          </Text>
        )}
        {isError && (
          <ApiErrorAlert
            error={error}
            fallback="Failed to load rule requests"
          />
        )}
        {!isLoading && visible.length === 0 && (
          <Text as="p" size="sm" className="text-ink-3">
            No requests.
          </Text>
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

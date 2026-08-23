import { type ReactNode, useState } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import {
  AccessRequestStatusPill,
  ApiErrorAlert,
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
  FormActions,
  SegmentedTabs,
  Text,
  deviceDisplayName,
  useAccessRequests,
  useDecideAccessRequest,
  useDevices,
} from "@wardnet/web";
import type {
  AccessRequestKind,
  AccessRequestStatus,
  ApprovalParams,
  Device,
  DeviceAccessRequest,
} from "@wardnet/js";

const ALL = "all";

/**
 * Per-kind UI, mirroring the daemon's approver registry.
 *
 * Approval is not uniform across request kinds, so neither is the UI for it:
 * each kind declares how it reads and what — if anything — the admin has to
 * decide before approving. Adding a kind means adding an entry here rather
 * than growing a `switch` inside the row component.
 */
interface AccessRequestKindUi {
  /** Card title. */
  title: string;
  /** What was asked for. `null` for kinds that name no subject. */
  subject: (req: DeviceAccessRequest) => ReactNode;
  /**
   * Extra control rendered before the Approve button, for kinds whose
   * approval needs an admin decision — the rule-auto-apply follow-up (#1197) plugs its
   * target-profile picker in here. `undefined` means approving is one click.
   */
  ApprovalControl?: (props: {
    req: DeviceAccessRequest;
    onChange: (params: ApprovalParams | undefined) => void;
  }) => ReactNode;
  /** Params sent with an approval when the kind needs no admin input. */
  defaultApproval?: ApprovalParams;
}

function DomainSubject({ req }: { req: DeviceAccessRequest }) {
  return (
    <Text size="sm" className="font-mono text-ink">
      {req.domain}
    </Text>
  );
}

const KIND_UI: Record<AccessRequestKind, AccessRequestKindUi> = {
  block: {
    title: "Block request",
    subject: (req) => <DomainSubject req={req} />,
  },
  allow: {
    title: "Allow request",
    subject: (req) => <DomainSubject req={req} />,
  },
  private_dns: {
    title: "Private DNS request",
    // A Private-DNS request names no subject — the device *is* the subject,
    // and it is already on the line below.
    subject: () => null,
    defaultApproval: { kind: "private_dns" },
  },
};

function deviceLabel(device: Device | undefined, deviceId: string): string {
  if (!device) return `Unknown device (${deviceId.slice(0, 8)})`;
  return deviceDisplayName(device);
}

function RequestRow({
  req,
  deviceName,
}: {
  req: DeviceAccessRequest;
  deviceName: string;
}) {
  const decide = useDecideAccessRequest();
  const ui = KIND_UI[req.kind];
  const [approval, setApproval] = useState<ApprovalParams | undefined>(
    ui.defaultApproval,
  );
  const pending = req.status === "pending";

  return (
    <Card data-testid={`access-request-${req.id}`}>
      <CardHeader>
        <CardTitle>{ui.title}</CardTitle>
        <CardAction>
          <AccessRequestStatusPill status={req.status} />
        </CardAction>
      </CardHeader>

      <CardContent className="flex flex-col gap-1">
        {ui.subject(req)}
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

        {pending && ui.ApprovalControl && (
          <ui.ApprovalControl req={req} onChange={setApproval} />
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
          secondaryLabel="Decline"
          secondaryProps={{
            onClick: () => decide.mutate({ id: req.id, status: "rejected" }),
            disabled: decide.isPending,
          }}
          primaryLabel="Approve"
          primaryProps={{
            onClick: () =>
              decide.mutate({ id: req.id, status: "approved", approval }),
            disabled: decide.isPending,
          }}
        />
      )}
    </Card>
  );
}

/**
 * Admin inbox for household access requests.
 *
 * What approval *does* depends on the kind: approving a Private-DNS request
 * mints the device's grant, while allow/block still only record the decision —
 * the admin applies the DNS rule in DNS Filtering.
 */
export default function AccessRequests() {
  const [filter, setFilter] = useState<AccessRequestStatus | undefined>(
    "pending",
  );
  // Fetch every request once and filter/count client-side (same pattern as the
  // DHCP table) so the tabs can show per-status counters.
  const { data, isLoading, isError, error } = useAccessRequests();
  const { data: devicesData } = useDevices();

  const deviceById = new Map(
    (devicesData?.devices ?? []).map((d) => [d.id, d]),
  );

  const all = data ?? [];
  const countOf = (s: AccessRequestStatus) =>
    all.filter((r) => r.status === s).length;
  const tabs = [
    { id: "pending", label: "Pending", count: countOf("pending") },
    { id: "approved", label: "Approved", count: countOf("approved") },
    { id: "rejected", label: "Declined", count: countOf("rejected") },
    { id: ALL, label: "All", count: all.length },
  ];
  const visible = filter ? all.filter((r) => r.status === filter) : all;

  return (
    <>
      <PageHeader
        title="Access requests"
        description="What household devices have asked for. Approving a Private DNS request grants it; for block / allow it records the decision - apply the rule in DNS Filtering."
      />

      <div className="flex flex-col gap-4">
        <SegmentedTabs
          tabs={tabs}
          activeId={filter ?? ALL}
          onChange={(id) =>
            setFilter(id === ALL ? undefined : (id as AccessRequestStatus))
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
            fallback="Failed to load access requests"
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

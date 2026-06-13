import { useState } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import {
  ApiErrorAlert,
  Button,
  Card,
  CardContent,
  Pill,
  useDecideRuleRequest,
  useRuleRequests,
} from "@wardnet/web";
import type { DeviceRuleRequest, RuleRequestStatus } from "@wardnet/js";

const FILTERS: { label: string; value: RuleRequestStatus | undefined }[] = [
  { label: "Pending", value: "pending" },
  { label: "Approved", value: "approved" },
  { label: "Rejected", value: "rejected" },
  { label: "All", value: undefined },
];

function statusPill(status: RuleRequestStatus) {
  if (status === "approved") return <Pill variant="ok">Approved</Pill>;
  if (status === "rejected") return <Pill variant="down">Rejected</Pill>;
  return <Pill variant="info">Pending</Pill>;
}

function RequestRow({ req }: { req: DeviceRuleRequest }) {
  const decide = useDecideRuleRequest();
  const pending = req.status === "pending";

  return (
    <Card>
      <CardContent className="flex flex-col gap-3 py-4">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <span className="font-mono text-sm text-ink">{req.domain}</span>
            <span className="block text-xs text-ink-3">
              {req.kind === "block" ? "Block request" : "Allow request"} ·
              device {req.device_id.slice(0, 8)} ·{" "}
              {new Date(req.created_at).toLocaleString()}
            </span>
          </div>
          {statusPill(req.status)}
        </div>

        {req.reason && (
          <p className="rounded-lg bg-sunken px-3 py-2 text-sm text-ink-2">
            “{req.reason}”
          </p>
        )}

        {pending && (
          <div className="flex gap-2">
            <Button
              size="sm"
              onClick={() => decide.mutate({ id: req.id, status: "approved" })}
              disabled={decide.isPending}
            >
              Approve
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => decide.mutate({ id: req.id, status: "rejected" })}
              disabled={decide.isPending}
            >
              Reject
            </Button>
          </div>
        )}

        {decide.isError && (
          <ApiErrorAlert
            error={decide.error}
            fallback="Failed to update request"
          />
        )}
      </CardContent>
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
  const { data, isLoading, isError, error } = useRuleRequests(filter);

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Rule requests"
        description="Block / allow requests from household devices. Approving records the decision — apply the rule in DNS Filtering."
      />

      <div className="flex gap-2">
        {FILTERS.map((f) => (
          <Button
            key={f.label}
            size="sm"
            variant={filter === f.value ? "default" : "outline"}
            onClick={() => setFilter(f.value)}
          >
            {f.label}
          </Button>
        ))}
      </div>

      {isLoading && <p className="text-sm text-ink-3">Loading…</p>}
      {isError && (
        <ApiErrorAlert error={error} fallback="Failed to load rule requests" />
      )}
      {data && data.length === 0 && (
        <p className="text-sm text-ink-3">No requests.</p>
      )}

      <div className="flex flex-col gap-3">
        {data?.map((req) => (
          <RequestRow key={req.id} req={req} />
        ))}
      </div>
    </div>
  );
}

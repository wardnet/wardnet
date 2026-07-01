import { useState } from "react";
import { Button } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Toggle } from "@wardnet/web";
import { ApiErrorAlert } from "@wardnet/web";
import {
  useDnsCaptureSettings,
  useUpdateDnsCaptureSettings,
} from "@wardnet/web";
import { formatBytes } from "@wardnet/web";

interface DeviceDnsCaptureCardProps {
  deviceId: string;
}

function StorageBar({ value, max }: { value: number; max: number }) {
  const pct = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0;
  const color = pct >= 90 ? "bg-danger" : pct >= 70 ? "bg-warn" : "bg-accent";
  return (
    <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-line">
      <div
        className={`h-full rounded-full transition-all ${color}`}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

export function DeviceDnsCaptureCard({ deviceId }: DeviceDnsCaptureCardProps) {
  const { data, isLoading } = useDnsCaptureSettings(deviceId);
  const update = useUpdateDnsCaptureSettings(deviceId);

  const [editing, setEditing] = useState(false);
  const [enabled, setEnabled] = useState(false);
  const [capCount, setCapCount] = useState(1000);
  const [capDays, setCapDays] = useState(7);

  function startEdit() {
    if (!data) return;
    setEnabled(data.enabled);
    setCapCount(data.cap_count);
    setCapDays(data.cap_days);
    update.reset();
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    update.reset();
  }

  async function handleSave() {
    await update.mutateAsync({
      enabled,
      cap_count: capCount,
      cap_days: capDays,
    });
    setEditing(false);
  }

  if (isLoading || !data) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>DNS capture</CardTitle>
        </CardHeader>
        <CardContent>
          <Text as="p" size="sm" className="text-ink-3">
            Loading…
          </Text>
        </CardContent>
      </Card>
    );
  }

  if (editing) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>DNS capture</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <Text as="p" size="xs" weight="medium" className="text-ink-3">
                Capture enabled
              </Text>
              <Text as="p" size="xs" className="mt-0.5 text-ink-3">
                Store DNS queries made by this device for review and analysis.
              </Text>
            </div>
            <Toggle
              checked={enabled}
              onCheckedChange={setEnabled}
              aria-label="Enable DNS capture"
            />
          </div>
          <div className="grid grid-cols-2 gap-x-4 gap-y-4">
            <div>
              <Text as="p" size="xs" weight="medium" className="text-ink-3">
                Max records
              </Text>
              <Text as="p" size="xs" className="mt-0.5 text-ink-3">
                Maximum events to retain.
              </Text>
              <Input
                type="number"
                min={1}
                value={capCount}
                onChange={(e) => setCapCount(Number(e.target.value))}
                className="mt-2"
              />
            </div>
            <div>
              <Text as="p" size="xs" weight="medium" className="text-ink-3">
                Retain days
              </Text>
              <Text as="p" size="xs" className="mt-0.5 text-ink-3">
                Delete events older than this.
              </Text>
              <Input
                type="number"
                min={1}
                value={capDays}
                onChange={(e) => setCapDays(Number(e.target.value))}
                className="mt-2"
              />
            </div>
          </div>
          {update.isError && <ApiErrorAlert error={update.error} />}
        </CardContent>
        <FormActions
          secondaryLabel="Cancel"
          secondaryProps={{
            onClick: cancelEdit,
            disabled: update.isPending,
          }}
          primaryLabel={update.isPending ? "Saving…" : "Save"}
          primaryProps={{
            onClick: handleSave,
            disabled: update.isPending,
          }}
        />
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>DNS capture</CardTitle>
        <CardAction>
          <Button variant="outline" size="sm" onClick={startEdit}>
            Edit
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <Text as="dl" size="sm" className="grid grid-cols-2 gap-x-6 gap-y-4">
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Status
            </Text>
            <dd>{data.enabled ? "Enabled" : "Disabled"}</dd>
          </div>
          <div>
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Retention
            </Text>
            <dd>
              {data.cap_count.toLocaleString()} records · {data.cap_days} days
            </dd>
          </div>
          <div className="col-span-2">
            <Text
              as="dt"
              size="xs"
              className="uppercase tracking-wide text-ink-3"
            >
              Storage
            </Text>
            <dd>
              {data.row_count.toLocaleString()} records ·{" "}
              {formatBytes(data.size_bytes)}
            </dd>
            <StorageBar value={data.row_count} max={data.cap_count} />
          </div>
        </Text>
      </CardContent>
    </Card>
  );
}

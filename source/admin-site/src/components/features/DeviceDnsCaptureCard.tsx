import { useState } from "react";
import { Button } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
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
          <p className="text-sm text-ink-3">Loading…</p>
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
          <Field label="Capture enabled">
            <Toggle
              checked={enabled}
              onCheckedChange={setEnabled}
              aria-label="Enable DNS capture"
            />
          </Field>
          <Field
            label="Max records"
            help="Maximum number of DNS events to retain."
          >
            <Input
              type="number"
              min={1}
              value={capCount}
              onChange={(e) => setCapCount(Number(e.target.value))}
              disabled={!enabled}
            />
          </Field>
          <Field
            label="Retain days"
            help="Delete events older than this many days."
          >
            <Input
              type="number"
              min={1}
              value={capDays}
              onChange={(e) => setCapDays(Number(e.target.value))}
              disabled={!enabled}
            />
          </Field>
          {update.isError && <ApiErrorAlert error={update.error} />}
        </CardContent>
        <CardFooter className="flex gap-2">
          <Button
            variant="default"
            size="sm"
            onClick={handleSave}
            disabled={update.isPending}
          >
            {update.isPending ? "Saving…" : "Save"}
          </Button>
          <Button variant="outline" size="sm" onClick={cancelEdit}>
            Cancel
          </Button>
        </CardFooter>
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
      <CardContent className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <span className="text-sm font-medium text-ink-2">Status</span>
          <span className="text-sm text-ink-1">
            {data.enabled ? "Enabled" : "Disabled"}
          </span>
        </div>
        <div className="flex flex-col gap-1">
          <span className="text-sm font-medium text-ink-2">Retention</span>
          <span className="text-sm text-ink-1">
            Up to {data.cap_count.toLocaleString()} records · {data.cap_days}{" "}
            days
          </span>
        </div>
        <div className="flex flex-col gap-1">
          <span className="text-sm font-medium text-ink-2">Stored</span>
          <span className="text-sm text-ink-1">
            {data.row_count.toLocaleString()} records ·{" "}
            {formatBytes(data.size_bytes)}
          </span>
        </div>
      </CardContent>
    </Card>
  );
}

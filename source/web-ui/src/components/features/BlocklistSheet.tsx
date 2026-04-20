import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Switch } from "@/components/core/ui/switch";
import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { CronSchedulePicker } from "@/components/compound/CronSchedulePicker";
import { useCreateBlocklist, useUpdateBlocklist } from "@/hooks/useDns";
import type { Blocklist } from "@wardnet/js";

interface BlocklistSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Blocklist to edit. When omitted, the sheet creates a new blocklist. */
  blocklist?: Blocklist | null;
}

const DEFAULT_SCHEDULE = "0 3 * * *";

/** Sheet form for creating or editing a domain blocklist. */
export function BlocklistSheet({ open, onOpenChange, blocklist }: BlocklistSheetProps) {
  const isEdit = !!blocklist;

  const createBlocklist = useCreateBlocklist();
  const updateBlocklist = useUpdateBlocklist();
  const mutation = isEdit ? updateBlocklist : createBlocklist;

  const [name, setName] = useState(blocklist?.name ?? "");
  const [url, setUrl] = useState(blocklist?.url ?? "");
  const [schedule, setSchedule] = useState(blocklist?.cron_schedule ?? DEFAULT_SCHEDULE);
  const [enabled, setEnabled] = useState(blocklist?.enabled ?? true);

  async function handleSave() {
    if (isEdit && blocklist) {
      await updateBlocklist.mutateAsync({
        id: blocklist.id,
        body: { name, url, cron_schedule: schedule, enabled },
      });
    } else {
      await createBlocklist.mutateAsync({ name, url, cron_schedule: schedule, enabled });
    }
    onOpenChange(false);
  }

  const canSave = name.trim() !== "" && url.trim() !== "";

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>{isEdit ? "Edit Blocklist" : "Add Blocklist"}</SheetTitle>
        <div className="mt-6 flex flex-col gap-5">
          <div className="flex flex-col gap-2">
            <Label htmlFor="bl-name">Name</Label>
            <Input
              id="bl-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Steven Black Hosts"
            />
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="bl-url">URL</Label>
            <Input
              id="bl-url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts"
            />
            <p className="text-xs text-muted-foreground">
              Hosts file, ABP/AdGuard list, or domain-per-line format.
            </p>
          </div>

          <CronSchedulePicker label="Update Schedule" value={schedule} onChange={setSchedule} />

          <div className="flex items-center justify-between">
            <Label htmlFor="bl-enabled">{isEdit ? "Enabled" : "Enable immediately"}</Label>
            <Switch id="bl-enabled" checked={enabled} onCheckedChange={setEnabled} />
          </div>

          {mutation.isError && (
            <ApiErrorAlert
              error={mutation.error}
              fallback={isEdit ? "Failed to update blocklist" : "Failed to add blocklist"}
            />
          )}

          <Button onClick={handleSave} disabled={mutation.isPending || !canSave} className="w-full">
            {mutation.isPending
              ? isEdit
                ? "Saving..."
                : "Adding..."
              : isEdit
                ? "Save Changes"
                : "Add Blocklist"}
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  );
}

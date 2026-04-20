import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useCreateAllowlistEntry } from "@/hooks/useDns";

interface CreateAllowlistSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Sheet form for adding a domain to the allowlist. */
export function CreateAllowlistSheet({ open, onOpenChange }: CreateAllowlistSheetProps) {
  const createEntry = useCreateAllowlistEntry();

  const [domain, setDomain] = useState("");
  const [reason, setReason] = useState("");

  async function handleSave() {
    await createEntry.mutateAsync({ domain, reason: reason || undefined });
    setDomain("");
    setReason("");
    onOpenChange(false);
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>Allow Domain</SheetTitle>
        <div className="mt-6 flex flex-col gap-5">
          <div className="flex flex-col gap-2">
            <Label htmlFor="al-domain">Domain</Label>
            <Input
              id="al-domain"
              value={domain}
              onChange={(e) => setDomain(e.target.value)}
              placeholder="example.com"
            />
            <p className="text-xs text-muted-foreground">
              This domain will never be blocked, even if it appears in a blocklist.
            </p>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="al-reason">Reason (optional)</Label>
            <Input
              id="al-reason"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Required for work VPN"
            />
          </div>

          {createEntry.isError && (
            <ApiErrorAlert error={createEntry.error} fallback="Failed to add allowlist entry" />
          )}

          <Button
            onClick={handleSave}
            disabled={createEntry.isPending || domain.trim() === ""}
            className="w-full"
          >
            {createEntry.isPending ? "Adding..." : "Allow Domain"}
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  );
}

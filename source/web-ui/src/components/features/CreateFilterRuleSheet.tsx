import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Switch } from "@/components/core/ui/switch";
import { Sheet, SheetContent, SheetTitle } from "@/components/core/ui/sheet";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useCreateFilterRule } from "@/hooks/useDns";

interface CreateFilterRuleSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Sheet form for adding a custom AdGuard-syntax filter rule. */
export function CreateFilterRuleSheet({ open, onOpenChange }: CreateFilterRuleSheetProps) {
  const createRule = useCreateFilterRule();

  const [ruleText, setRuleText] = useState("");
  const [comment, setComment] = useState("");
  const [enabled, setEnabled] = useState(true);

  async function handleSave() {
    await createRule.mutateAsync({ rule_text: ruleText, comment: comment || undefined, enabled });
    setRuleText("");
    setComment("");
    setEnabled(true);
    onOpenChange(false);
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>Add Filter Rule</SheetTitle>
        <div className="mt-6 flex flex-col gap-5">
          <div className="flex flex-col gap-2">
            <Label htmlFor="fr-rule">Rule</Label>
            <Input
              id="fr-rule"
              value={ruleText}
              onChange={(e) => setRuleText(e.target.value)}
              placeholder="||ads.example.com^"
              className="font-mono"
            />
            <p className="text-xs text-muted-foreground">
              AdGuard syntax — e.g. <code className="font-mono">||ads.example.com^</code> to block,{" "}
              <code className="font-mono">@@||example.com^</code> to allow.
            </p>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="fr-comment">Comment (optional)</Label>
            <Input
              id="fr-comment"
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              placeholder="Block tracking pixel"
            />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="fr-enabled">Enable immediately</Label>
            <Switch id="fr-enabled" checked={enabled} onCheckedChange={setEnabled} />
          </div>

          {createRule.isError && (
            <ApiErrorAlert error={createRule.error} fallback="Failed to add filter rule" />
          )}

          <Button
            onClick={handleSave}
            disabled={createRule.isPending || ruleText.trim() === ""}
            className="w-full"
          >
            {createRule.isPending ? "Adding..." : "Add Rule"}
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  );
}

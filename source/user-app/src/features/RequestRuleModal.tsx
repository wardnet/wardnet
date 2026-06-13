import { useEffect, useRef, useState } from "react";
import {
  Button,
  Drawer,
  DrawerContent,
  DrawerDescription,
  DrawerTitle,
  Textarea,
  useCreateRuleRequest,
} from "@wardnet/web";
import type { RuleRequestKind } from "@wardnet/js";

export interface RequestTarget {
  domain: string;
  kind: RuleRequestKind;
}

/**
 * Bottom sheet that lets a household user ask the admin to block or allow a
 * domain. Opened from a domain row (or activity event) in the Stats page; the
 * row decides the sensible default (unblock a blocked domain, block a queried
 * one). Uses the same bottom-Drawer pattern as the admin app's sheets.
 */
export function RequestRuleModal({
  target,
  onClose,
}: {
  target: RequestTarget | null;
  onClose: () => void;
}) {
  const create = useCreateRuleRequest();
  const [kind, setKind] = useState<RuleRequestKind>("block");
  const [reason, setReason] = useState("");

  // Latch the last target so the closing animation can finish after the parent
  // clears it.
  const latched = useRef<RequestTarget | null>(null);
  if (target !== null) latched.current = target;
  const active = latched.current;

  // Reset the form whenever a new domain is targeted.
  useEffect(() => {
    if (target) {
      setKind(target.kind);
      setReason("");
    }
  }, [target]);

  const open = target !== null;

  function submit() {
    if (!active) return;
    create.mutate(
      { kind, domain: active.domain, reason: reason.trim() || null },
      { onSuccess: onClose },
    );
  }

  return (
    <Drawer open={open} onOpenChange={(next) => !next && onClose()}>
      <DrawerContent side="bottom">
        <div className="mx-auto mt-3 mb-4 h-1 w-10 rounded-full bg-line" />
        <div
          className="flex flex-col gap-4 px-4"
          style={{ paddingBottom: "max(24px, env(safe-area-inset-bottom))" }}
        >
          <div className="flex flex-col gap-1">
            <DrawerTitle className="text-base font-semibold text-ink">
              Ask your administrator
            </DrawerTitle>
            <DrawerDescription className="text-sm text-ink-3">
              Send a request about{" "}
              <span className="font-mono text-ink">{active?.domain}</span>. Your
              administrator decides whether to apply it.
            </DrawerDescription>
          </div>

          {/* Neutral segmented control — the accent is reserved for the
              single primary CTA ("Send request") below. */}
          <div className="flex gap-1 rounded-xl bg-sunken p-1">
            {(["block", "allow"] as const).map((k) => (
              <button
                key={k}
                type="button"
                onClick={() => setKind(k)}
                aria-pressed={kind === k}
                className={`flex-1 rounded-lg py-2 text-sm font-medium transition-colors ${
                  kind === k
                    ? "bg-card text-ink shadow-sm"
                    : "text-ink-3 active:text-ink"
                }`}
              >
                {k === "block" ? "Block it" : "Allow it"}
              </button>
            ))}
          </div>

          <Textarea
            placeholder="Add a note for your administrator (optional)"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={3}
          />

          <div className="flex gap-2">
            <Button variant="outline" className="flex-1" onClick={onClose}>
              Cancel
            </Button>
            <Button
              className="flex-1"
              onClick={submit}
              disabled={create.isPending}
            >
              {create.isPending ? "Sending…" : "Send request"}
            </Button>
          </div>
        </div>
      </DrawerContent>
    </Drawer>
  );
}

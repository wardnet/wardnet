import { useRef, useState } from "react";
import {
  Drawer,
  DrawerContent,
  DrawerTitle,
  Text,
  Button,
  Toggle,
} from "@wardnet/web";
import {
  useSetInboundWgPeerEnabled,
  useRemoveInboundWgPeer,
} from "@wardnet/web";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import type { InboundWgPeerSummary } from "@wardnet/js";

interface Props {
  peer: InboundWgPeerSummary | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Operational peer management (issue #813): pause/resume (keeps the
 *  credential) and revoke (deletes it). No QR/config here — the private
 *  key only ever exists at grant time. */
export function InboundWgPeerSheet({ peer, open, onOpenChange }: Props) {
  const setEnabled = useSetInboundWgPeerEnabled();
  const removePeer = useRemoveInboundWgPeer();
  const [revokeOpen, setRevokeOpen] = useState(false);

  // Keep the last non-null peer so the exit animation can complete even if
  // the parent clears the selection while the sheet is closing.
  const latchedRef = useRef<InboundWgPeerSummary | null>(null);
  if (peer !== null) latchedRef.current = peer;
  const activePeer = latchedRef.current;

  if (!activePeer) return null;

  return (
    <>
      <Drawer open={open} onOpenChange={onOpenChange}>
        <DrawerContent side="bottom" aria-describedby={undefined}>
          <div className="mx-auto mt-3 mb-4 h-1 w-10 rounded-full bg-line" />
          <DrawerTitle className="px-4 pb-1 text-[11px] font-semibold uppercase tracking-wider text-ink-3">
            Remote access — {activePeer.name}
          </DrawerTitle>
          <div
            className="flex flex-col gap-4 px-4"
            style={{ paddingBottom: "max(24px, env(safe-area-inset-bottom))" }}
          >
            <div className="flex items-center justify-between gap-3 rounded-xl border border-line bg-card p-4">
              <div>
                <Text as="p" size="base" weight="medium" className="text-ink">
                  {activePeer.enabled ? "Active" : "Paused"}
                </Text>
                <Text as="p" size="xs" className="mt-0.5 text-ink-3">
                  Pausing keeps the credential — resuming needs no new QR scan.
                </Text>
              </div>
              <Toggle
                aria-label="Enable this peer"
                checked={activePeer.enabled}
                disabled={setEnabled.isPending}
                onCheckedChange={(next) =>
                  setEnabled.mutate({ id: activePeer.id, enabled: next })
                }
              />
            </div>
            <Button
              variant="destructive"
              onClick={() => setRevokeOpen(true)}
              disabled={removePeer.isPending}
            >
              Revoke access
            </Button>
          </div>
        </DrawerContent>
      </Drawer>

      <ConfirmDialog
        open={revokeOpen}
        onOpenChange={setRevokeOpen}
        title="Revoke remote access"
        description={`This permanently deletes ${activePeer.name}'s credential. Re-granting later needs a fresh QR scan.`}
        confirmLabel="Revoke"
        onConfirm={() => {
          removePeer.mutate(activePeer.id, {
            onSuccess: () => onOpenChange(false),
          });
          setRevokeOpen(false);
        }}
      />
    </>
  );
}

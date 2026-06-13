import { useEffect, useState } from "react";
import {
  Button,
  Modal,
  ModalBody,
  ModalContent,
  ModalDescription,
  ModalFooter,
  ModalHeader,
  ModalTitle,
  Textarea,
  useCreateRuleRequest,
} from "@wardnet/web";
import type { RuleRequestKind } from "@wardnet/js";

export interface RequestTarget {
  domain: string;
  kind: RuleRequestKind;
}

/**
 * Bottom-of-flow modal that lets a household user ask the admin to block or
 * allow a domain. Opened from a domain row in the Stats page; the row decides
 * the sensible default (unblock a blocked domain, block a queried one).
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

  // Reset the form whenever a new domain is targeted.
  useEffect(() => {
    if (target) {
      setKind(target.kind);
      setReason("");
    }
  }, [target]);

  const open = target !== null;

  function submit() {
    if (!target) return;
    create.mutate(
      { kind, domain: target.domain, reason: reason.trim() || null },
      { onSuccess: onClose },
    );
  }

  return (
    <Modal open={open} onOpenChange={(next) => !next && onClose()}>
      <ModalContent>
        <ModalHeader>
          <ModalTitle>Ask your administrator</ModalTitle>
          <ModalDescription>
            Send a request about{" "}
            <span className="font-mono text-ink">{target?.domain}</span>. Your
            administrator decides whether to apply it.
          </ModalDescription>
        </ModalHeader>
        <ModalBody className="flex flex-col gap-4">
          <div className="flex gap-2">
            <Button
              type="button"
              variant={kind === "block" ? "default" : "outline"}
              className="flex-1"
              onClick={() => setKind("block")}
            >
              Block it
            </Button>
            <Button
              type="button"
              variant={kind === "allow" ? "default" : "outline"}
              className="flex-1"
              onClick={() => setKind("allow")}
            >
              Allow it
            </Button>
          </div>
          <Textarea
            placeholder="Add a note for your administrator (optional)"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={3}
          />
        </ModalBody>
        <ModalFooter>
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={submit} disabled={create.isPending}>
            {create.isPending ? "Sending…" : "Send request"}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

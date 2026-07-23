import {
  Modal,
  ModalContent,
  ModalBody,
  ModalFooter,
  ModalTitleBlock,
  Button,
  PrivateDnsInstructions,
  privateDnsService,
} from "@wardnet/web";
import type { PrivateDnsGrantSummary } from "@wardnet/js";

interface PrivateDnsGrantedModalProps {
  /** `null` closes the modal. */
  grant: PrivateDnsGrantSummary | null;
  /** Human name for the granted device, shown in the title. */
  deviceName: string;
  /** Fire the device-keyed setup push. */
  onSendToDevice: (grant: PrivateDnsGrantSummary) => void;
  /** Whether a send is in flight (disables the button). */
  sending: boolean;
  onDismiss: () => void;
}

/**
 * Shown right after a grant. Unlike the WireGuard modal there is no one-time
 * secret here — the hostname is stable and also lives in the grant table — so
 * this is a convenience surface: the admin can push the setup link to the
 * device, or read the hostname/instructions off to the household member.
 */
export function PrivateDnsGrantedModal({
  grant,
  deviceName,
  onSendToDevice,
  sending,
  onDismiss,
}: PrivateDnsGrantedModalProps) {
  if (!grant) return null;

  return (
    <Modal open onOpenChange={(next) => !next && onDismiss()}>
      <ModalContent>
        <ModalTitleBlock
          title={`Private DNS granted - ${deviceName}`}
          description="Set this up on the device below, or send it straight there. The hostname stays available in the grants table."
        />
        <ModalBody>
          <PrivateDnsInstructions
            hostname={grant.hostname}
            profileUrl={privateDnsService.profileUrl()}
          />
        </ModalBody>
        <ModalFooter>
          <Button
            variant="outline"
            onClick={() => onSendToDevice(grant)}
            disabled={sending}
            data-testid="private-dns-modal-send"
          >
            {sending ? "Sending…" : "Send to device"}
          </Button>
          <Button onClick={onDismiss}>Done</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

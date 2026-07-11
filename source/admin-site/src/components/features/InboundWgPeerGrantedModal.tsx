import { Modal, ModalContent, ModalBody, ModalFooter } from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { InboundWgQrCode, ModalTitleBlock } from "@wardnet/web";
import { peerConfigFilename, triggerBrowserDownload } from "@wardnet/web";
import type { AddInboundWgPeerResponse } from "@wardnet/js";

interface InboundWgPeerGrantedModalProps {
  /** `null` closes the modal. The private key inside `client_config` is the
   *  only copy that will ever exist client- or server-side. Once dismissed
   *  it's gone. */
  peer: AddInboundWgPeerResponse | null;
  onDismiss: () => void;
}

/** Shown exactly once, immediately after a successful grant. The daemon
 *  assembles the full client config (private key included) and returns it in
 *  this one response. It is never persisted server-side and can't be shown
 *  again, so this is the admin's only chance to scan the QR or download the
 *  `.conf`. */
export function InboundWgPeerGrantedModal({
  peer,
  onDismiss,
}: InboundWgPeerGrantedModalProps) {
  if (!peer) return null;

  const config = peer.client_config;

  function downloadConfig() {
    if (!config) return;
    const blob = new Blob([config], { type: "text/plain" });
    triggerBrowserDownload(blob, `${peerConfigFilename(peer!.name)}.conf`);
  }

  return (
    <Modal open onOpenChange={(next) => !next && onDismiss()}>
      <ModalContent>
        <ModalTitleBlock
          title={`Remote access granted - ${peer.name}`}
          description="In the WireGuard app, choose Add, then Scan from QR code, or import the downloaded config file. This is the only time the private key is shown; it is never stored on the daemon."
        />
        <ModalBody className="flex flex-col items-center gap-4">
          {config ? (
            <>
              <InboundWgQrCode value={config} />
              <Text as="p" size="xs" className="text-center text-ink-3">
                Scan from inside the WireGuard app. The phone camera just shows
                text, since a WireGuard config isn&apos;t a link.
              </Text>
            </>
          ) : (
            <Text as="p" size="sm" className="text-danger">
              No public hostname or IP is known yet. Set up remote access (DDNS)
              first, then the config can include a reachable endpoint.
            </Text>
          )}
        </ModalBody>
        <ModalFooter className="flex-col gap-2 sm:flex-row sm:justify-end">
          <Button variant="outline" onClick={downloadConfig} disabled={!config}>
            Download .conf
          </Button>
          <Button onClick={onDismiss}>I&apos;ve saved this</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

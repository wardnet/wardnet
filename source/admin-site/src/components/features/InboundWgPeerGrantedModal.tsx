import {
  Modal,
  ModalContent,
  ModalHeader,
  ModalTitle,
  ModalDescription,
  ModalBody,
  ModalFooter,
} from "@wardnet/web";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { InboundWgQrCode } from "@wardnet/web";
import { buildInboundWgClientConfig } from "@wardnet/web";
import type { AddInboundWgPeerResponse } from "@wardnet/js";

interface InboundWgPeerGrantedModalProps {
  /** `null` closes the modal. The private key here is the only copy that
   *  will ever exist client- or server-side — once dismissed it's gone. */
  peer: AddInboundWgPeerResponse | null;
  serverPublicKey: string;
  /** Placeholder `host:port` — see the caveat on the endpoint field the
   *  client config is built from. `null` when neither a DDNS hostname nor
   *  a last-known public IP is available yet. */
  endpoint: string | null;
  onDismiss: () => void;
}

/** Shown exactly once, immediately after a successful grant — the private
 *  key in the response is never persisted server-side and can't be shown
 *  again, so this is the admin's only chance to scan the QR or download
 *  the `.conf`. */
export function InboundWgPeerGrantedModal({
  peer,
  serverPublicKey,
  endpoint,
  onDismiss,
}: InboundWgPeerGrantedModalProps) {
  if (!peer) return null;

  const config = endpoint
    ? buildInboundWgClientConfig({
        privateKey: peer.private_key,
        allowedIp: peer.allowed_ip,
        serverPublicKey,
        endpoint,
      })
    : null;

  function downloadConfig() {
    if (!config) return;
    const blob = new Blob([config], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${peer!.name.replace(/\s+/g, "-").toLowerCase()}.conf`;
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <Modal open onOpenChange={(next) => !next && onDismiss()}>
      <ModalContent>
        <ModalHeader>
          <ModalTitle>Remote access granted — {peer.name}</ModalTitle>
          <ModalDescription>
            Scan this QR code on the device's WireGuard app, or download the
            config file. This is the only time the private key is shown — it is
            never stored on the daemon.
          </ModalDescription>
        </ModalHeader>
        <ModalBody className="flex flex-col items-center gap-4">
          {config ? (
            <InboundWgQrCode value={config} />
          ) : (
            <Text as="p" size="sm" className="text-danger">
              No public hostname or IP is known yet — set up remote access
              (DDNS) first, then the config can include a reachable endpoint.
            </Text>
          )}
          <Text as="p" size="xs" className="text-center text-ink-3">
            Endpoint is a placeholder using this network's public hostname/IP —
            full relay wiring is tracked separately and may not yet be reachable
            from outside the LAN.
          </Text>
        </ModalBody>
        <ModalFooter className="flex-col gap-2 sm:flex-row sm:justify-end">
          <Button variant="outline" onClick={downloadConfig} disabled={!config}>
            Download .conf
          </Button>
          <Button onClick={onDismiss}>I've saved this</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

import { Text, Button, InboundWgQrCode } from "@wardnet/web";
import { peerConfigFilename, triggerBrowserDownload } from "@wardnet/web";
import type { AddInboundWgPeerResponse } from "@wardnet/js";

interface Props {
  /** The one-time grant response. Its `client_config` embeds the private key
   *  and is never re-fetchable, so this view is the only chance to scan it. */
  peer: AddInboundWgPeerResponse;
  onDone: () => void;
}

/** The post-grant QR / `.conf` view, shared by every mobile grant entry point.
 *  The daemon assembles the full client config; this just renders it. */
export function InboundWgGrantedView({ peer, onDone }: Props) {
  const config = peer.client_config;

  function download() {
    if (!config) return;
    const blob = new Blob([config], { type: "text/plain" });
    triggerBrowserDownload(blob, `${peerConfigFilename(peer.name)}.conf`);
  }

  return (
    <div className="flex flex-col items-center gap-4 px-4 pb-2">
      {config ? (
        <>
          <InboundWgQrCode value={config} size={220} />
          <Text as="p" size="xs" className="text-center text-ink-3">
            In the WireGuard app, tap Add, then Scan from QR code. The phone
            camera just shows text, since a WireGuard config isn&apos;t a link.
          </Text>
          <Text as="p" size="xs" className="text-center text-ink-3">
            This is the only time the private key is shown; it is never stored
            on the daemon.
          </Text>
          <div className="flex w-full flex-col gap-2">
            <Button variant="outline" className="w-full" onClick={download}>
              Download .conf
            </Button>
            <Button className="w-full" onClick={onDone}>
              I&apos;ve saved this
            </Button>
          </div>
        </>
      ) : (
        <>
          <Text as="p" size="sm" className="text-center text-danger">
            No public hostname or IP is known yet. Set up remote access first to
            get a usable config.
          </Text>
          <Button className="w-full" onClick={onDone}>
            Close
          </Button>
        </>
      )}
    </div>
  );
}

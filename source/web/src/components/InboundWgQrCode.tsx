import { useEffect, useState } from "react";
import { toDataURL } from "qrcode";

interface InboundWgQrCodeProps {
  /** The full WireGuard client `.conf` text to encode. */
  value: string;
  size?: number;
  className?: string;
}

/** QR rendering for an inbound-WireGuard peer's client config (issues
 *  #812-#813), shared so admin-site and admin-app render identically. */
export function InboundWgQrCode({
  value,
  size = 240,
  className,
}: InboundWgQrCodeProps) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    toDataURL(value, { width: size, margin: 1 })
      .then((url) => {
        if (!cancelled) setDataUrl(url);
      })
      .catch(() => {
        if (!cancelled) setDataUrl(null);
      });
    return () => {
      cancelled = true;
    };
  }, [value, size]);

  if (!dataUrl) {
    return (
      <div
        className={className}
        style={{ width: size, height: size }}
        aria-hidden
      />
    );
  }

  return (
    <img
      src={dataUrl}
      alt="WireGuard client config QR code"
      width={size}
      height={size}
      className={className}
    />
  );
}

import { useEffect, useState } from "react";
import { toDataURL } from "qrcode";

interface InboundWgQrCodeProps {
  /** The full WireGuard client `.conf` text to encode. */
  value: string;
  size?: number;
  className?: string;
  /** Accessible label for the rendered image. */
  alt?: string;
}

/** QR rendering for an inbound-WireGuard peer's client config (issues
 *  #812-#813), shared so admin-site and admin-app render identically. The
 *  `alt` default keeps existing callers unchanged; other QR uses (e.g. the
 *  Private DNS iOS profile URL) pass their own label. */
export function InboundWgQrCode({
  value,
  size = 240,
  className,
  alt = "WireGuard client config QR code",
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
      alt={alt}
      width={size}
      height={size}
      className={className}
    />
  );
}

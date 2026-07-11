/**
 * Build an `<a download>` link, click it, then release the object URL. The
 * anchor is appended to the DOM before `.click()` because some browsers
 * ignore a click on a detached anchor. The Blob never touches disk outside
 * the browser's own download flow.
 *
 * Shared by backup export and inbound-WireGuard `.conf` download so both
 * behave identically.
 */
export function triggerBrowserDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

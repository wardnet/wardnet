/** Detect iOS Safari outside an installed (standalone) PWA, where Web Push is
 * unavailable until the app is added to the Home Screen. */
export function isIosBrowserTab(): boolean {
  // iPadOS 13+ Safari sends a desktop "Macintosh" UA by default; a Mac with a
  // multi-touch screen does not exist, so maxTouchPoints disambiguates.
  const iOS =
    /iPad|iPhone|iPod/.test(navigator.userAgent) ||
    (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1);
  const standalone =
    typeof window.matchMedia === "function" &&
    window.matchMedia("(display-mode: standalone)").matches;
  return iOS && !standalone;
}

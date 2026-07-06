import { useEffect } from "react";
import { useLocation, useNavigationType } from "react-router";

/**
 * Scrolls the window to the top when navigating to a new page.
 *
 * `BrowserRouter` in declarative mode has no built-in scroll restoration
 * (that's only wired up for the data router via `<ScrollRestoration>`),
 * so without this, navigating to a new page keeps the previous page's
 * scroll offset, landing mid-page instead of at the top.
 *
 * Skipped on `POP` (back/forward navigation) so the browser's native
 * scroll-position restoration for those still applies.
 *
 * Uses `behavior: "instant"` to override the site-wide `scroll-behavior:
 * smooth` (`index.css`, meant for same-page anchor links) — a page-to-page
 * route change should snap to the top, not visibly animate the scroll.
 */
export function ScrollToTop() {
  const { pathname } = useLocation();
  const navigationType = useNavigationType();

  useEffect(() => {
    if (navigationType === "POP") return;
    window.scrollTo({ top: 0, left: 0, behavior: "instant" });
  }, [pathname, navigationType]);

  return null;
}

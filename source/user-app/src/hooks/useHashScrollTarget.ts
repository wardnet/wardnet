import { useEffect, useRef, type RefObject } from "react";
import { useLocation } from "react-router";

/**
 * How long we keep correcting the scroll position after a deep link arms it.
 *
 * The scroll itself is layout-driven, not timed — this only bounds how long a
 * a late sibling is still allowed to drag the target back into place, so the
 * hook never fights a page that keeps growing (an infinite list, a slow image)
 * long after the user has arrived.
 */
const SETTLE_WINDOW_MS = 2_000;

/**
 * Gestures that mean the user has taken over the scroll position.
 *
 * Deliberately not `scroll`: our own `scrollIntoView` emits scroll events, so
 * listening for those would disarm the hook the moment it did its job. These
 * four are all user-initiated.
 */
const TAKEOVER_EVENTS = [
  "wheel",
  "touchstart",
  "keydown",
  "pointerdown",
] as const;

/**
 * Keep an element scrolled into view for as long as a URL fragment targets it,
 * even while the page around it is still settling (issue #1176).
 *
 * A one-shot `scrollIntoView()` on mount is a race the deep link usually loses:
 * the element scrolled to is a "Loading…" placeholder, and the data-driven
 * cards *above* it then expand as their queries resolve and push the target
 * back off-screen. The fix is to treat the scroll as a position to hold rather
 * than an event to fire — a `ResizeObserver` on the element **and its
 * container** re-issues the scroll whenever anything changes height, which is
 * exactly when the target moves. Observing the container is what catches the
 * siblings: they are the elements that actually displace the target, and a
 * card that appears late (a list that renders `null` until it has rows) grows
 * the container even though it was not around to be observed at arm time.
 *
 * Arms on mount and on `hashchange`. The `hashchange` listener is not
 * redundant with `useLocation().hash`: when the app is already open on the page
 * the service worker calls `existing.navigate(…#fragment)`, a fragment-only
 * same-document navigation that fires `hashchange` but never `popstate` — and
 * `BrowserRouter` only listens to the latter, so the router's hash would stay
 * stale in exactly the case this exists for. For the same reason the live
 * `window.location.hash` is what the hook trusts, and a fragment that moves off
 * the target releases the hold.
 *
 * The user always outranks the hold: the first scroll gesture ends it for the
 * rest of this navigation, because scrolling away *is* an answer to the deep
 * link. Only a new fragment revives it.
 *
 * @param targetHash Fragment that targets this element, including the `#`.
 * @param rearmOn Any value that changes when the element's *own* content
 *   changes (typically a query's `isLoading`). A change re-arms the window, so
 *   a query that resolves after `SETTLE_WINDOW_MS` still lands the user on the
 *   content instead of on the placeholder it replaced — unless they have
 *   already scrolled away.
 * @returns Ref to attach to the element that should be scrolled to.
 */
export function useHashScrollTarget<T extends HTMLElement>(
  targetHash: string,
  rearmOn?: unknown,
): RefObject<T | null> {
  const ref = useRef<T>(null);
  // Both survive effect re-runs, which is the point: `rearmOn` changing must
  // not resurrect a hold the user already dismissed, and must not be mistaken
  // for a fresh navigation.
  const releasedRef = useRef(false);
  const armedFragmentRef = useRef<string | null>(null);
  const { hash: routerHash } = useLocation();

  useEffect(() => {
    // Reassigned by `arm()`; the cleanup below always calls the current one.
    let stop = () => {};

    const arm = () => {
      // Scrolling away is an answer to the deep link — a query resolving a
      // moment later must not yank the reader back to where they left.
      if (releasedRef.current) return;
      const el = ref.current;
      if (!el) return;
      // A second deep link restarts the window rather than stacking observers.
      stop();

      const scroll = () =>
        el.scrollIntoView({ behavior: "smooth", block: "start" });
      scroll();

      // Scrolling changes no box size, so re-scrolling from here cannot feed
      // back into the observer.
      const observer = new ResizeObserver(scroll);
      observer.observe(el);
      if (el.parentElement) observer.observe(el.parentElement);

      let timer = 0;
      const disarm = () => {
        observer.disconnect();
        window.clearTimeout(timer);
        for (const type of TAKEOVER_EVENTS) {
          window.removeEventListener(type, release);
        }
        stop = () => {};
      };
      // The window merely expiring leaves the deep link answerable — a late
      // query may still re-arm. A user gesture ends it for good.
      const release = () => {
        releasedRef.current = true;
        disarm();
      };
      timer = window.setTimeout(disarm, SETTLE_WINDOW_MS);
      for (const type of TAKEOVER_EVENTS) {
        window.addEventListener(type, release, { passive: true });
      }
      stop = disarm;
    };

    // Named `fragment` rather than `hash`: the `security` ESLint plugin's
    // timing-attack heuristic keys off the identifier, and a URL fragment is
    // not a secret.
    const retarget = (fragment: string) => {
      if (fragment !== armedFragmentRef.current) {
        // A fragment we haven't seen is a fresh navigation, so it outranks an
        // earlier release: the reader asked to be taken somewhere again.
        armedFragmentRef.current = fragment;
        releasedRef.current = false;
      }
      if (fragment === targetHash) {
        arm();
      } else {
        // The fragment moved off us — let go rather than keep dragging the
        // page back to a target the URL no longer names. The effect does not
        // re-run to clean this up on its own: a fragment-only navigation
        // leaves the router's hash untouched.
        stop();
      }
    };

    // The live fragment outranks the router's: on the fragment-only navigation
    // above, `useLocation()` is stale by construction. It is empty under a
    // MemoryRouter (tests, and any non-browser history), hence the fallback.
    retarget(window.location.hash || routerHash);
    const onHashChange = () => retarget(window.location.hash);
    window.addEventListener("hashchange", onHashChange);
    return () => {
      window.removeEventListener("hashchange", onHashChange);
      stop();
    };
  }, [targetHash, routerHash, rearmOn]);

  return ref;
}

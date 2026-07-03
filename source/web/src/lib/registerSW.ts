export interface RegisterSWOptions {
  /** Path to the compiled service-worker file. Defaults to `"sw.js"` (vite-plugin-pwa default). */
  swPath?: string;
  /**
   * Auto-apply a waiting service worker instead of waiting for the app to call
   * `updateSW()`: skip-waiting the new worker, which triggers one page reload so
   * the fresh app shell takes over. A first install never reloads (there is no
   * prior controller to replace). Use for appliance surfaces that should always
   * match the daemon after an upgrade. When `false` (default), a waiting worker
   * is surfaced through `onNeedRefresh` for the app to prompt instead.
   */
  immediate?: boolean;
  /** Called when a new SW version is waiting to take over. Call the returned `updateSW()` fn to apply it. */
  onNeedRefresh?: () => void;
  /** Called when the SW has installed for the first time and the app is ready to serve requests offline. */
  onOfflineReady?: () => void;
  /** Called once the SW is registered successfully. */
  onRegistered?: (registration: ServiceWorkerRegistration) => void;
  /** Called if registration throws. */
  onRegisterError?: (error: unknown) => void;
}

/**
 * Registers the app's own service worker and wires up update + offline-ready callbacks.
 *
 * Matches the contract of vite-plugin-pwa's `virtual:pwa-register` so consuming apps
 * can keep the same call-site regardless of whether they use the plugin's auto-register
 * or this helper. Designed to work with the `injectManifest` strategy.
 *
 * Returns a function that posts `SKIP_WAITING` to the pending SW and reloads the page,
 * or `undefined` when service workers are not supported by the browser.
 */
export function registerSW(
  options: RegisterSWOptions = {},
): (() => void) | undefined {
  if (!("serviceWorker" in navigator)) return undefined;

  const {
    swPath = "sw.js",
    immediate = false,
    onNeedRefresh,
    onOfflineReady,
    onRegistered,
    onRegisterError,
  } = options;

  let waitingWorker: ServiceWorker | null = null;

  const updateSW = () => {
    waitingWorker?.postMessage({ type: "SKIP_WAITING" });
  };

  // A newly-installed worker is waiting to take over. Auto-apply it when
  // `immediate` (skip-waiting → `controllerchange` → one reload); otherwise
  // surface it via `onNeedRefresh` for the app to prompt. Shared by the
  // "already waiting at register time" and "just finished installing" paths.
  const applyWaiting = (worker: ServiceWorker) => {
    waitingWorker = worker;
    if (immediate) {
      updateSW();
    } else {
      onNeedRefresh?.();
    }
  };

  // Registered once — outside the `.then()` — so it fires exactly once even if
  // `register()` is called multiple times (e.g. HMR in dev). The `refreshing`
  // guard prevents double-reloads within the same listener.
  let refreshing = false;
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (!refreshing) {
      refreshing = true;
      window.location.reload();
    }
  });

  const register = () => {
    navigator.serviceWorker
      .register(swPath)
      .then((registration) => {
        onRegistered?.(registration);

        // A worker may already be waiting if it installed in a prior session and
        // was never activated — apply it too, so auto-update clients aren't
        // stranded on the stale shell across reloads.
        if (registration.waiting) {
          applyWaiting(registration.waiting);
        }

        registration.addEventListener("updatefound", () => {
          const installing = registration.installing;
          if (!installing) return;

          installing.addEventListener("statechange", () => {
            if (installing.state !== "installed") return;

            if (navigator.serviceWorker.controller) {
              // An existing SW is active — this is an update waiting to activate.
              applyWaiting(installing);
            } else {
              // First install — app is now ready to serve requests offline.
              onOfflineReady?.();
            }
          });
        });
      })
      .catch((error: unknown) => {
        onRegisterError?.(error);
      });
  };

  // If the page is already fully loaded (e.g. helper called lazily), register immediately.
  if (document.readyState === "complete") {
    register();
  } else {
    window.addEventListener("load", register, { once: true });
  }

  return updateSW;
}

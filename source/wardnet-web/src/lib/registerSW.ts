export interface RegisterSWOptions {
  /** Path to the compiled service-worker file. Defaults to `"sw.js"` (vite-plugin-pwa default). */
  swPath?: string;
  /** Skip the waiting SW and reload immediately when an update is found. Default: `false`. */
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
export function registerSW(options: RegisterSWOptions = {}): (() => void) | undefined {
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

  const register = () => {
    navigator.serviceWorker
      .register(swPath)
      .then((registration) => {
        onRegistered?.(registration);

        // A worker may already be waiting if the user refreshed while an update was pending.
        if (registration.waiting) {
          waitingWorker = registration.waiting;
          onNeedRefresh?.();
        }

        registration.addEventListener("updatefound", () => {
          const installing = registration.installing;
          if (!installing) return;

          installing.addEventListener("statechange", () => {
            if (installing.state !== "installed") return;

            if (navigator.serviceWorker.controller) {
              // An existing SW is active — this is an update waiting to activate.
              waitingWorker = installing;
              if (immediate) {
                updateSW();
              } else {
                onNeedRefresh?.();
              }
            } else {
              // First install — app is now ready to serve requests offline.
              onOfflineReady?.();
            }
          });
        });

        // Reload once the new SW takes control so all assets are served from the updated cache.
        let refreshing = false;
        navigator.serviceWorker.addEventListener("controllerchange", () => {
          if (!refreshing) {
            refreshing = true;
            window.location.reload();
          }
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

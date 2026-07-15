/// <reference lib="webworker" />
import {
  cleanupOutdatedCaches,
  createHandlerBoundToURL,
  precacheAndRoute,
} from "workbox-precaching";
import { NavigationRoute, registerRoute } from "workbox-routing";
import { registerPushHandlers } from "@wardnet/web/sw";

declare const self: ServiceWorkerGlobalScope;

cleanupOutdatedCaches();
precacheAndRoute(self.__WB_MANIFEST);

// Web Push (issues #482/#764): show daemon notifications and deep-link on
// click (e.g. a NewDeviceQuarantined tap lands on /admin-app/devices).
registerPushHandlers(self, {
  icon: "icons/admin-192.png",
  badge: "icons/badge-96.png",
});

// Offline shell — navigation requests fall back to the app-shell index.
// Navigations are dispatched to the SW whose scope contains the destination
// URL, so only `/admin-app/…` ones ever reach this route — no denylist
// needed for the other surfaces or `/api`.
const handler = createHandlerBoundToURL("/admin-app/index.html");
registerRoute(new NavigationRoute(handler));

self.addEventListener("message", (event) => {
  if (event.data?.type === "SKIP_WAITING") {
    self.skipWaiting();
  }
});

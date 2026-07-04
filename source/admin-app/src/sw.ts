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

// Offline shell — all navigation requests fall back to the app-shell index
const handler = createHandlerBoundToURL("/admin-app/index.html");
const navigationRoute = new NavigationRoute(handler, {
  denylist: [/^\/api/],
});
registerRoute(navigationRoute);

self.addEventListener("message", (event) => {
  if (event.data?.type === "SKIP_WAITING") {
    self.skipWaiting();
  }
});

/// <reference lib="webworker" />
import { cleanupOutdatedCaches, createHandlerBoundToURL, precacheAndRoute } from "workbox-precaching";
import { NavigationRoute, registerRoute } from "workbox-routing";

declare const self: ServiceWorkerGlobalScope;

cleanupOutdatedCaches();
precacheAndRoute(self.__WB_MANIFEST);

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

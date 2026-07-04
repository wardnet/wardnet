/// <reference lib="webworker" />
import {
  cleanupOutdatedCaches,
  createHandlerBoundToURL,
  precacheAndRoute,
} from "workbox-precaching";
import { NavigationRoute, registerRoute } from "workbox-routing";

declare const self: ServiceWorkerGlobalScope;

cleanupOutdatedCaches();
precacheAndRoute(self.__WB_MANIFEST);

// Offline shell — navigation requests fall back to the app-shell index.
// This SW's scope is the origin root, but the daemon serves three surfaces
// on one origin (user PWA at /, admin site at /admin/, admin PWA at
// /admin-app/) plus API/health endpoints — denylist everything that is not
// this app, or their navigations get hijacked with the user-app shell.
const handler = createHandlerBoundToURL("/index.html");
const navigationRoute = new NavigationRoute(handler, {
  denylist: [/^\/api/, /^\/admin/, /^\/health/],
});
registerRoute(navigationRoute);

self.addEventListener("message", (event) => {
  if (event.data?.type === "SKIP_WAITING") {
    self.skipWaiting();
  }
});

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

// Web Push (issue #594): show device-keyed daemon notifications ("Routing
// locked"/"Routing changed") and focus-or-open the app on tap. Device
// notifications carry no deep link, so clicks land on the app root.
registerPushHandlers(self, {
  icon: "icons/user-192.png",
  badge: "icons/badge-96.png",
});

// Offline shell — navigation requests fall back to the app-shell index.
// This SW's scope is `/app/` (sibling to `/admin-app/`, so the two PWAs are
// installable side by side); only navigations inside that scope ever reach
// it. The `/api` denylist mirrors the admin-app SW as belt and braces.
const handler = createHandlerBoundToURL("/app/index.html");
const navigationRoute = new NavigationRoute(handler, {
  denylist: [/^\/api/],
});
registerRoute(navigationRoute);

self.addEventListener("message", (event) => {
  if (event.data?.type === "SKIP_WAITING") {
    self.skipWaiting();
  }
});

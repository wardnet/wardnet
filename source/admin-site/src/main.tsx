import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ConnectionGate } from "@wardnet/web";
import App from "./App";
import { Toaster } from "@wardnet/ui";
import { LogStreamManager } from "@/services/LogStreamManager";
import { DnsLogStreamManager } from "@/services/DnsLogStreamManager";
import "./index.css";

// Capture a same-origin `returnTo` query parameter set by other Wardnet
// surfaces (e.g. the admin-app's setup interstitial) so the wizard can
// redirect back after setup completes. The value persists in sessionStorage
// so it survives internal wizard navigations that change the URL.
(function captureReturnTo() {
  const params = new URLSearchParams(window.location.search);
  const raw = params.get("returnTo");
  if (!raw) return;
  try {
    const u = new URL(raw, window.location.origin);
    if (u.origin === window.location.origin) {
      sessionStorage.setItem("wardnet_returnTo", u.pathname + u.search);
    }
  } catch {
    // malformed URL — ignore silently
  }
})();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter basename="/admin">
        <ConnectionGate>
          <App />
          <Toaster />
          <LogStreamManager />
          <DnsLogStreamManager />
        </ConnectionGate>
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
);

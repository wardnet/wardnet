import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Toaster } from "@wardnet/ui";
import { ConnectionGate, registerSW } from "@wardnet/web";
import App from "./App";
import "./index.css";

// `immediate` auto-updates the precached shell after a daemon upgrade so clients
// aren't stranded on a stale build (see `RegisterSWOptions.immediate`).
registerSW({ swPath: "/admin-app/sw.js", immediate: true });

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
      <BrowserRouter basename="/admin-app">
        <ConnectionGate>
          <App />
          <Toaster />
        </ConnectionGate>
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
);

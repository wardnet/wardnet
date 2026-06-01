import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { registerSW } from "@wardnet/wardnet-web";
import App from "./App";
import "./index.css";

registerSW({ swPath: "/admin-app/sw.js" });

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter basename="/admin-app">
      <App />
    </BrowserRouter>
  </StrictMode>,
);

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app/App";
import "@/styles/index.css";

// Development preview with synthetic data (see src/dev/preview.ts).
if (import.meta.env.DEV && new URLSearchParams(location.search).has("preview")) {
  await import("./dev/preview");
}

const root = document.getElementById("root");
if (!root) throw new Error("Root element #root not found");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

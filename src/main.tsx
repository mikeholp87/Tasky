import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app/App";
import "@/styles/index.css";

// Preview with synthetic data (see src/dev/preview.ts). Dev builds opt in with
// `?preview`; the website demo is a production build made with VITE_DEMO=1
// (scripts/build-demo.sh) and always runs on the synthetic data.
const demo = import.meta.env.VITE_DEMO === "1";
if (demo || (import.meta.env.DEV && new URLSearchParams(location.search).has("preview"))) {
  await import("./dev/preview");
}

const root = document.getElementById("root");
if (!root) throw new Error("Root element #root not found");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

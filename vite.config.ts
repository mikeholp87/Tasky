/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { blaVitePlugin } from "bla-agents/bundler";
import process from "node:process";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/  —  https://v2.tauri.app/start/frontend/vite/
export default defineConfig(({ command, mode }) => ({
  plugins: [
    react(),
    tailwindcss(),
    ...(command === "build" && mode !== "test" && process.env.BLA_VITE_SHIELD === "1"
      ? [
          blaVitePlugin({
            intensity: "balanced",
            seed: "tasky-bla-v2-shield",
            include: /(?:^|\/)assets\/index-[^/]+\.js$/i,
            honeySourceMap: false,
          }),
        ]
      : []),
  ],
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "src") },
  },
  // Keep Rust compiler errors visible in the terminal.
  clearScreen: false,
  server: {
    // Tauri expects a fixed port.
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes("node_modules")) {
            return "vendor";
          }
          return undefined;
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: false,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
}));

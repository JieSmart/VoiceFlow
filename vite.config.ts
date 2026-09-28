import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  optimizeDeps: {
    include: [
      "react",
      "react-dom",
      "react-dom/client",
      "zustand",
      "clsx",
      "@phosphor-icons/react",
      "@tauri-apps/api/window",
      "@tauri-apps/api/core",
      "@tauri-apps/api/event",
    ],
  },
  server: { port: 1420, strictPort: true, host: "0.0.0.0", allowedHosts: ["terminal.local"] },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        bubble: "bubble.html",
      },
    },
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});

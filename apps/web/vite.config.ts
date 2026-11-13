import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    // LiveKit is lazy-loaded on voice join and intentionally isolated.
    chunkSizeWarningLimit: 700,
  },
  server: {
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:18080",
      "/gateway": {
        target: "ws://127.0.0.1:18080",
        ws: true,
      },
    },
  },
});

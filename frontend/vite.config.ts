/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Entwicklung: `npm run dev` leitet /api an ein lokal laufendes Backend weiter (TRACKCRAFT_DEV_BACKEND).
const backend = process.env.TRACKCRAFT_DEV_BACKEND ?? "https://127.0.0.1:8443";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: { outDir: "dist", sourcemap: false, assetsInlineLimit: 0 },
  server: { proxy: { "/api": { target: backend, changeOrigin: false, secure: false } } },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    coverage: {
      provider: "v8",
      // Ziel nach RFC-0014: Logikordner mindestens 70 % Zeilen, Gate erst ab M6.
      include: ["src/utils/**", "src/api/**", "src/hooks/**"],
      reporter: ["text-summary", "lcov"],
    },
  },
});

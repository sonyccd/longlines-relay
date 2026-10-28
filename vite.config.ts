/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri loads the dev server from this fixed port (tauri.conf.json `build.devUrl`).
const DEV_PORT = 1420;

export default defineConfig({
  plugins: [react()],
  // Tauri prints its own errors; keep Vite's output from clearing them.
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    // The Rust side is rebuilt by `tauri dev`; Vite need not watch it.
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  build: {
    // The webviews Tauri 2 supports: WebView2 (Chromium), WKWebView on macOS 10.15+ and WebKitGTK.
    target: ["es2022", "chrome105", "safari15"],
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
  },
});

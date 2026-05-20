import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { codeInspectorPlugin } from "code-inspector-plugin";

export default defineConfig(({ command }) => ({
  root: "src",
  plugins: [
    command === "serve" &&
      codeInspectorPlugin({
        bundler: "vite",
      }),
    react(),
  ].filter(Boolean),
  base: "./",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
  server: {
    port: 3000,
    strictPort: true,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@tauri-apps/api/core": path.resolve(__dirname, "./src/tauri-shim/core.ts"),
      "@tauri-apps/api/event": path.resolve(__dirname, "./src/tauri-shim/event.ts"),
      "@tauri-apps/api/app": path.resolve(__dirname, "./src/tauri-shim/app.ts"),
      "@tauri-apps/api/path": path.resolve(__dirname, "./src/tauri-shim/path.ts"),
      "@tauri-apps/api/window": path.resolve(__dirname, "./src/tauri-shim/window.ts"),
      "@tauri-apps/plugin-dialog": path.resolve(__dirname, "./src/tauri-shim/plugin-dialog.ts"),
      "@tauri-apps/plugin-process": path.resolve(__dirname, "./src/tauri-shim/plugin-process.ts"),
    },
  },
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_"],
}));


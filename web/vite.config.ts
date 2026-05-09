import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import path from "node:path";

// Vite config for the Journal web POC.
// - React + JSX automatic runtime
// - `@/*` alias mirrors tsconfig paths so editor + bundler agree
// - Dev port 5173 (Vite default); change with `pnpm dev -- --port=…`
// - `vite-plugin-wasm` handles `.wasm` URLs emitted by wasm-bindgen's
//   `--target web` output. `vite-plugin-top-level-await` is its
//   companion for browsers without TLA support (Vite 5 enables ES2022
//   by default which already has TLA, but the plugin is cheap insurance
//   for older targets).
export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
  server: {
    port: 5173,
  },
  build: {
    outDir: "dist",
    sourcemap: true,
    target: "es2022",
  },
  // wasm-bindgen output uses native ESM dynamic imports; Vite's worker
  // helper / pre-bundler doesn't need to crawl `generated/` to find them.
  optimizeDeps: {
    exclude: [
      "@/wasm/generated/shim/journal_web_shim.js",
      "@/wasm/generated/viewer/journal_web_viewer.js",
    ],
  },
});

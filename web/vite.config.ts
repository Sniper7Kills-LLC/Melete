import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// Vite config for the Journal web POC.
// - React + JSX automatic runtime
// - `@/*` alias mirrors tsconfig paths so editor + bundler agree
// - Dev port 5173 (Vite default); change with `pnpm dev -- --port=…`
export default defineConfig({
  plugins: [react()],
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
  },
});

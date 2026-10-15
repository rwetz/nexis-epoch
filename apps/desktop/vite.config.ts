/// <reference types="vitest/config" />
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Tauri expects a fixed dev port and must not have Vite watching its own
// build output.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // `@epoch`, not `@`: Nexis builds this source into its Epoch window, and
  // its own sources already own `@`.
  resolve: { alias: { "@epoch": fileURLToPath(new URL("./src", import.meta.url)) } },
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: {
    target: "es2022",
    chunkSizeWarningLimit: 2500,
    rollupOptions: {
      output: {
        // Keep the heavy, rarely-changing libraries in their own chunks.
        manualChunks: {
          echarts: ["echarts"],
          codemirror: ["@codemirror/state", "@codemirror/view", "@codemirror/language", "@codemirror/lang-sql"],
        },
      },
    },
  },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});

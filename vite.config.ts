import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri 需要固定端口，且不监听 src-tauri 目录
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WKWebView / WebView2 均为现代内核；es2020 兼顾 macOS 10.15 的 Safari 13
    target: "es2020",
    minify: "esbuild",
    sourcemap: false,
    chunkSizeWarningLimit: 800,
  },
});
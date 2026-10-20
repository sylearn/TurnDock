import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

// Tauri 开发服务器：固定端口，不清屏，便于看到 Rust 日志。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { port: 5199, strictPort: true, host: '127.0.0.1' },
  build: { target: 'safari16', sourcemap: false, cssMinify: true },
  test: { environment: 'happy-dom' },
})

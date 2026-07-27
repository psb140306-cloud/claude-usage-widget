import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { resolve } from 'node:path'

const host = process.env.TAURI_DEV_HOST

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],

  // 멀티 윈도우: 위젯(index.html)과 설정(settings.html)을 별도 엔트리로 빌드한다.
  // Tauri 쪽 window.url 이 각각을 가리킨다 (src-tauri/tauri.conf.json).
  build: {
    target: 'esnext', // WebView2 Evergreen 만 대상으로 하므로 다운레벨 불필요
    rollupOptions: {
      input: {
        widget: resolve(__dirname, 'index.html'),
        settings: resolve(__dirname, 'settings.html'),
      },
    },
  },

  // Tauri 개발 시 필요한 설정
  clearScreen: false, // Rust 컴파일 에러를 가리지 않는다
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
})

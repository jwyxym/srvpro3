import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import vueDevTools from 'vite-plugin-vue-devtools'

// https://vite.dev/config/
export default defineConfig(({ mode }) => ({
  define: {
    __DEV__: mode === 'development',
  },
  plugins: [
    vue(),
    vueDevTools(),
  ],
  server: {
    proxy: {
      '/cards': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
      '/host': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
      '/room': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
      '/history': {
        target: 'http://127.0.0.1:8080',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://127.0.0.1:8080',
        ws: true,
        changeOrigin: true,
      },
    },
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
}))

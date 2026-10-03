import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: { port: 5173, strictPort: true, watch: { ignored: ['**/src-tauri/**', '**/crates/**'] } },
  build: { target: ['es2022', 'chrome105', 'safari14'] },
  clearScreen: false,
})

import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// Dev mode: `vite dev` serves the React app with hot module reload and
// proxies `/api` (REST + the `/api/v1/events` SSE stream) to the real
// `vesica serve` process, so UI work never needs a Rust rebuild per change.
// Point VESICA_DEV_API_PROXY_TARGET at a different host:port if the backend
// isn't running on its default bind address; see web/README.md.
const apiProxyTarget =
  process.env.VESICA_DEV_API_PROXY_TARGET ?? 'http://127.0.0.1:8787'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': {
        target: apiProxyTarget,
        changeOrigin: true,
        // The SSE stream (`GET /api/v1/events`) is a long-lived response;
        // the proxy must not buffer/rewrite it away.
        ws: false,
      },
    },
  },
})

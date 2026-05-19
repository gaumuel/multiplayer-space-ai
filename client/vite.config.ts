import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/.well-known/webtransport': {
        target: 'https://localhost:4433',
        changeOrigin: true,
        secure: false,
      },
    },
  },
});

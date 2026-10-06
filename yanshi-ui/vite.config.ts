import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  base: '/ui/',
  build: {
    rollupOptions: {
      output: {
        assetFileNames: 'assets/styles.css',
        chunkFileNames: 'assets/app.js',
        entryFileNames: 'assets/app.js',
      },
    },
  },
  plugins: [react()],
});

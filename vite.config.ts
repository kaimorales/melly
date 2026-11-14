import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react()], clearScreen: false,
  server: { strictPort: true, port: 1420, host: '127.0.0.1' },
  // The worker is lazy at runtime. Pre-bundle its imports in dev so the first
  // code block doesn't trigger Vite's dependency-discovery page reload.
  optimizeDeps: { include: ['lowlight', ...['python', 'javascript', 'typescript', 'rust', 'go', 'bash', 'json', 'xml', 'css', 'sql', 'yaml', 'java', 'cpp', 'swift', 'markdown'].map((language) => `highlight.js/lib/languages/${language}`)] },
  build: { target: 'es2022', sourcemap: false },
});

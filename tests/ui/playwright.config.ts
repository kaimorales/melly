import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.ts',
  fullyParallel: false,
  reporter: 'list',
  use: { baseURL: 'http://127.0.0.1:1420', viewport: { width: 1472, height: 816 }, colorScheme: 'dark' },
  webServer: {
    command: 'npm run web -- --host 127.0.0.1',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});

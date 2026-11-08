import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.mjs',
  testIgnore: 'platform.spec.mjs',
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  reporter: [['list'], ['json', { outputFile: fileURLToPath(new URL('../test-results/results.json', import.meta.url)) }]],
  outputDir: '../test-results/artifacts',
  use: {
    baseURL: 'http://127.0.0.1:4178',
    viewport: { width: 1440, height: 1100 },
    headless: true,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: process.env.BROWSER_EXECUTABLE ? { executablePath: process.env.BROWSER_EXECUTABLE } : {},
  },
  webServer: {
    command: 'python3 -m http.server 4178 --bind 127.0.0.1 --directory dist',
    cwd: fileURLToPath(new URL('..', import.meta.url)),
    url: 'http://127.0.0.1:4178/nibli-learn/',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});

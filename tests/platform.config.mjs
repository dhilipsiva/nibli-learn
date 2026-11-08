import { defineConfig } from '@playwright/test';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  testDir: '.', testMatch: 'platform.spec.mjs', fullyParallel: false, workers: 1,
  timeout: 60_000, expect: { timeout: 10_000 },
  reporter: [['list'], ['json', { outputFile: fileURLToPath(new URL('../test-results/platform-results.json', import.meta.url)) }]],
  outputDir: '../test-results/platform-artifacts',
  use: { baseURL: 'http://127.0.0.1:4181', headless: true, trace: 'retain-on-failure', screenshot: 'only-on-failure',
    launchOptions: process.env.BROWSER_EXECUTABLE ? { executablePath: process.env.BROWSER_EXECUTABLE } : {} },
  webServer: { command: 'python3 tests/serve_platform.py', cwd: fileURLToPath(new URL('..', import.meta.url)),
    url: 'http://127.0.0.1:4181/nibli-learn/', reuseExistingServer: false, timeout: 30_000 },
});

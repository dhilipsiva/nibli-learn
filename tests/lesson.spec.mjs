import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { runReleaseJourney, workspace, localView, openLesson, ready, run } from './helpers/release-journey.mjs';

test('the real seven-step lesson connects prediction, evidence, replay, proof and repair', async ({ page }, testInfo) => {
  await runReleaseJourney(page, testInfo);
});

test('reset is explicit, themes and responsive reading preserve access', async ({ page }, testInfo) => {
  await openLesson(page, 3);
  await page.getByTestId('composer-text').fill('approves(Reviewer, BuildA).');
  await page.getByTestId('lesson-reset').click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect(page.getByRole('button', { name: 'Keep working', exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(page.getByTestId('composer-text')).toHaveValue('approves(Reviewer, BuildA).');
  await page.getByTestId('lesson-reset').click();
  await page.getByTestId('confirm-reset').click();
  await expect(dialog).toBeHidden();
  await expect(page.getByTestId('composer-text')).toHaveValue('');
  await ready(page);
  await page.getByTestId('theme-toggle').click();
  await expect(page.locator('.app')).toHaveAttribute('data-theme', 'light');
  await page.screenshot({ path: testInfo.outputPath('lesson-light-desktop.png'), fullPage: true });
  await page.setViewportSize({ width: 900, height: 1100 });
  await expect(page.getByTestId('composer-text')).toBeVisible();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('lesson-light-tablet.png'), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByText('Reading mode on this screen', { exact: true })).toBeVisible();
  await expect(page.getByTestId('composer-text')).toBeHidden();
  await expect(page.getByTestId('run-claim')).toBeHidden();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('lesson-light-reading.png'), fullPage: true });
});

test('progress export and validated replacement keep accepted knowledge separate', async ({ page }, testInfo) => {
  await openLesson(page);
  await run(page, 'FALSE');
  const before = await workspace(page);
  await page.getByRole('link', { name: 'Local data · no account · no analytics', exact: true }).click();
  const downloadEvent = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export progress', exact: true }).click();
  const download = await downloadEvent;
  const filename = testInfo.outputPath('progress.json');
  await download.saveAs(filename);
  const progress = JSON.parse(await readFile(filename, 'utf8'));
  expect(progress).toEqual(before.progress);
  expect(progress.session).toBeUndefined();
  expect(JSON.stringify(progress)).not.toContain('approves(');
  await page.getByLabel('Exported JSON', { exact: true }).fill(JSON.stringify({ ...progress, schema: 999999 }));
  await page.getByRole('button', { name: 'Validate and preview', exact: true }).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Apply this validated import', exact: true })).toHaveCount(0);
  expect((await workspace(page)).progress).toEqual(before.progress);
  progress.lessons['found.investigate-release']['1']['release.one-approval']['1'].checked = false;
  await page.getByLabel('Exported JSON', { exact: true }).fill(JSON.stringify(progress));
  await page.getByRole('button', { name: 'Validate and preview', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Apply this validated import', exact: true })).toBeVisible();
  expect((await workspace(page)).progress).toEqual(before.progress);
  await page.getByRole('button', { name: 'Apply this validated import', exact: true }).click();
  await expect.poll(async () => (await workspace(page)).progress).toEqual(progress);
  const after = await workspace(page);
  expect(after.projects).toEqual(before.projects);
  await expect(page.getByRole('button', { name: 'Undo last replacement', exact: true })).toBeEnabled();
});

test('unreadable stored data is retained while the learner can still investigate', async ({ page }) => {
  const malformed = '{unreadable existing learner work';
  await page.addInitScript(value => localStorage.setItem('nibli-learn.local.v1', value), malformed);
  await openLesson(page);
  await expect(page.getByText('Saved state could not be read.', { exact: false }).first()).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('nibli-learn.local.v1'))).toBe(malformed);
  await run(page, 'FALSE');
  expect(await page.evaluate(() => localStorage.getItem('nibli-learn.local.v1'))).toBe(malformed);
  await page.getByRole('link', { name: 'Local data · no account · no analytics', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Export retained raw data', exact: true })).toBeVisible();
});

test('cancellation keeps aborted metadata and a delayed old-generation response cannot complete a new run', async ({ page }) => {
  await page.addInitScript(() => {
    const NativeWorker = globalThis.Worker;
    const transport = globalThis.transport = { workers: [], responses: [], hold: false, held: [], bypass: false };
    globalThis.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args);
        transport.workers.push(this);
        this.addEventListener('message', event => {
          if (transport.bypass) return;
          const response = typeof event.data === 'string' ? JSON.parse(event.data) : event.data;
          if (response.kind === 'phase') return;
          transport.responses.push(structuredClone(response));
          if (transport.hold) {
            event.stopImmediatePropagation();
            transport.held.push({ worker: this, response });
          }
        });
      }
      postMessage(message, ...rest) {
        transport.lastRequest = typeof message === 'string' ? JSON.parse(message) : message;
        return super.postMessage(message, ...rest);
      }
    };
    transport.deliver = (worker, response) => {
      transport.bypass = true;
      try { worker.dispatchEvent(new MessageEvent('message', { data: response })); }
      finally { transport.bypass = false; }
    };
  });
  await openLesson(page);
  const committed = (await localView(page)).session;
  await page.getByTestId('prediction-false').check();
  // Cancel through the real control as soon as the pending request renders it.
  // No artificial outcome or engine phase is substituted for this operation.
  await page.evaluate(() => new Promise((resolve, reject) => {
    const timer = setTimeout(() => { observer.disconnect(); reject(new Error('Cancel control was never rendered')); }, 5000);
    const observer = new MutationObserver(() => {
      const button = document.querySelector('[data-testid="run-cancel"]');
      if (button) { observer.disconnect(); clearTimeout(timer); button.click(); resolve(); }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    document.querySelector('[data-testid="run-claim"]').click();
  }));
  await ready(page);
  await expect(page.getByTestId('outcome')).toHaveAttribute('data-status', 'No completed outcome');
  await expect(page.getByTestId('outcome')).toHaveAttribute('data-run-id', '');
  await expect(page.getByTestId('aborted-attempt')).toHaveCount(1);
  const saved = await localView(page);
  expect(saved.session).toEqual(committed);
  expect(saved.aborted[0]).toMatchObject({ status: 'aborted', claim: 'ready(BuildA, Release).', epoch: committed.epoch, revision: committed.revision, settings: committed.settings });
  expect(saved.aborted[0].outcome).toBeUndefined();
  expect(saved.aborted[0].proof).toBeUndefined();

  await page.evaluate(() => { globalThis.transport.hold = true; });
  await page.getByTestId('run-claim').click();
  await expect.poll(() => page.evaluate(() => globalThis.transport.held.length)).toBe(1);
  await page.evaluate(() => {
    const transport = globalThis.transport;
    // Transport fault injection: replay an actual earlier reply but give it the
    // current request ID. Only generation checking can reject this stale envelope.
    const stale = structuredClone(transport.responses[0]);
    if (stale.generation === transport.lastRequest.generation) throw new Error('Worker generation did not advance');
    stale.id = transport.lastRequest.id;
    transport.deliver(transport.workers.at(-1), stale);
  });
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'busy');
  await expect(page.getByTestId('outcome')).toHaveAttribute('data-status', 'Deciding');
  await page.evaluate(() => {
    const transport = globalThis.transport;
    transport.hold = false;
    for (const { worker, response } of transport.held.splice(0)) transport.deliver(worker, response);
  });
  await ready(page);
  await expect(page.getByTestId('outcome')).toHaveAttribute('data-status', 'FALSE');
  await expect(page.getByTestId('outcome')).not.toHaveAttribute('data-run-id', '');
});

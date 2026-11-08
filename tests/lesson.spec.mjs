import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const lesson = '/nibli-learn/#/lesson/found.investigate-release/step/1';
const policy = 'all $build: approves(TestRunner, $build) & approves(Reviewer, $build) -> ready($build, Release).';

async function openLesson(page, step = 1) {
  await page.addInitScript(() => {
    const NativeWorker = globalThis.Worker;
    globalThis.workerMessages = [];
    globalThis.Worker = class extends NativeWorker {
      postMessage(message, ...rest) {
        const data = typeof message === 'string' ? JSON.parse(message) : message;
        globalThis.workerMessages.push({ id: data.id, command: data.command?.kind });
        return super.postMessage(message, ...rest);
      }
    };
  });
  await page.goto(lesson.replace('/step/1', `/step/${step}`));
  await ready(page);
  await expect(page.getByTestId(`step-${step}`)).toHaveAttribute('aria-current', 'step');
}
async function ready(page) {
  await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'ready');
}
async function run(page, prediction, outcome = prediction) {
  const result = page.getByTestId('outcome');
  const previous = await result.getAttribute('data-run-id');
  if (prediction) await page.getByTestId(`prediction-${prediction.toLowerCase()}`).check();
  await page.getByTestId('run-claim').click();
  await expect(result).toHaveAttribute('data-status', outcome);
  await expect.poll(() => result.getAttribute('data-run-id')).not.toBe(previous);
  await ready(page);
}
async function suggested(page) {
  await page.getByTestId('use-suggestion').click();
  await page.getByTestId('composer-assert').click();
  await expect(page.getByTestId('composer-text')).toHaveValue('');
  await ready(page);
}
async function next(page, step) {
  await page.getByTestId('next-step').click();
  await expect(page.getByTestId(`step-${step}`)).toHaveAttribute('aria-current', 'step');
  await ready(page);
}
async function checked(page, step) {
  await expect(page.getByTestId(`step-${step}`).getByLabel('Checked', { exact: true })).toBeVisible();
}

test('the real seven-step lesson connects prediction, evidence, replay, proof and repair', async ({ page }, testInfo) => {
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await openLesson(page);
  await expect(page.getByTestId('run-claim')).toBeDisabled();
  await expect(page.locator('[data-statement="approves(TestRunner, BuildA)."]')).toHaveAttribute('data-active', 'true');
  await run(page, 'FALSE');
  await checked(page, 1);

  await next(page, 2);
  await suggested(page);
  await run(page, 'FALSE');
  await checked(page, 2);

  await next(page, 3);
  await suggested(page);
  await run(page, 'TRUE');
  await checked(page, 3);

  await next(page, 4);
  await run(page, null, 'TRUE');
  const requestCount = await page.evaluate(() => globalThis.workerMessages.length);
  const originalRunId = await page.getByTestId('outcome').getAttribute('data-run-id');
  await page.getByTestId('replay-tab').click();
  const position = await page.getByTestId('replay-position').textContent();
  await page.getByTestId('replay-next').click();
  await expect(page.getByTestId('replay-position')).not.toHaveText(position);
  await page.getByTestId('replay-prev').click();
  await expect(page.getByTestId('replay-position')).toHaveText(position);
  await page.getByTestId('proof-tab').click();
  await expect(page.getByTestId('proof-export')).toBeVisible();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const downloadEvent = page.waitForEvent('download');
  await page.getByTestId('proof-export').click();
  const download = await downloadEvent;
  const proofFile = testInfo.outputPath('real-proof.json');
  await download.saveAs(proofFile);
  const exported = JSON.parse(await readFile(proofFile, 'utf8'));
  expect(exported.kind).toBe('proof-record');
  expect(exported.run.id).toBe(originalRunId);
  expect(exported.run.outcome.status).toBe('TRUE');
  expect(exported.run.record.events.length).toBeGreaterThan(0);
  expect(exported.run.proof_text.length).toBeGreaterThan(0);
  expect(exported.run.record).not.toEqual(exported.run.proof);
  await expect(page.getByTestId('outcome')).toHaveAttribute('data-run-id', originalRunId);
  expect(await page.evaluate(() => globalThis.workerMessages.length)).toBe(requestCount);
  await page.getByTestId('quiz-0').check();
  await page.getByTestId('check-understanding').click();
  await checked(page, 4);
  await page.screenshot({ path: testInfo.outputPath('proof-dark-desktop.png'), fullPage: true });

  await next(page, 5);
  await suggested(page);
  await run(page, 'TRUE');
  const reviews = page.locator('[data-statement="approves(Reviewer, BuildA)."][data-active="true"]');
  await expect(reviews).toHaveCount(2);
  await reviews.filter({ hasText: 'Review A1' }).getByRole('button', { name: 'Withdraw', exact: true }).click();
  await expect(reviews).toHaveCount(1);
  await run(page, 'TRUE');
  await reviews.filter({ hasText: 'Review A2' }).getByRole('button', { name: 'Withdraw', exact: true }).click();
  await expect(reviews).toHaveCount(0);
  await run(page, 'FALSE');
  await checked(page, 5);

  await next(page, 6);
  const revisionBeforeRefusal = await page.locator('.footer').getByText(/Input revision/).textContent();
  await page.getByTestId('use-suggestion').click();
  await page.getByTestId('composer-assert').click();
  await expect(page.getByTestId('admission-refusal')).toBeVisible();
  await expect(page.locator('.footer').getByText(/Input revision/)).toHaveText(revisionBeforeRefusal);
  await page.getByTestId('quiz-0').check();
  await page.getByTestId('check-understanding').click();
  await checked(page, 6);

  await next(page, 7);
  await page.getByTestId('open-exercise').click();
  await page.getByTestId('run-cases').click();
  await expect(page.getByTestId('case-results')).toContainText('Differs');
  await page.getByTestId('policy-editor').fill(policy);
  await page.getByTestId('apply-policy').click();
  await page.getByTestId('run-cases').click();
  await expect(page.getByTestId('case-results').getByText('✓ Matches', { exact: true })).toHaveCount(4);
  const rows = page.getByTestId('case-results').locator('tbody tr');
  for (let index = 0; index < 4; index++) {
    const cells = rows.nth(index).locator('td');
    const expected = (await cells.nth(1).textContent()).trim();
    await expect(cells.nth(2).locator('[data-status]')).toHaveAttribute('data-status', expected);
  }
  await page.getByTestId('repair-explanation').fill('Both approvals must apply to the same build. AND needs both conditions. The cross-build control detects a rule that matches approvals for different builds.');
  await page.getByTestId('check-repair').click();
  await checked(page, 7);
  await expect(page.getByText('Behavior verified: all four cases match.', { exact: false })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('repair-dark-desktop.png'), fullPage: true });
  const progress = await page.evaluate(() => JSON.parse(localStorage.getItem('nibli-learn.local.v1')).progress);
  expect(progress.steps.every(step => step.checked)).toBe(true);
  expect(errors).toEqual([]);
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
  const before = await page.evaluate(() => JSON.parse(localStorage.getItem('nibli-learn.local.v1')));
  await page.getByRole('link', { name: 'Local data · no account · no analytics', exact: true }).click();
  const downloadEvent = page.waitForEvent('download');
  await page.getByTestId('progress-export').click();
  const download = await downloadEvent;
  const filename = testInfo.outputPath('progress.json');
  await download.saveAs(filename);
  const progress = JSON.parse(await readFile(filename, 'utf8'));
  expect(progress).toEqual(before.progress);
  expect(progress.session).toBeUndefined();
  expect(JSON.stringify(progress)).not.toContain('approves(');
  const invalid = { ...progress, schema: 999999 };
  await page.getByTestId('progress-import').fill(JSON.stringify(invalid));
  await page.getByTestId('progress-validate').click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByTestId('progress-import-confirm')).toHaveCount(0);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('nibli-learn.local.v1')).progress)).toEqual(before.progress);
  progress.steps[0].checked = false;
  await page.getByTestId('progress-import').fill(JSON.stringify(progress));
  await page.getByTestId('progress-validate').click();
  await expect(page.getByTestId('progress-import-confirm')).toBeVisible();
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem('nibli-learn.local.v1')).progress)).toEqual(before.progress);
  await page.getByTestId('progress-import-confirm').click();
  await expect(page.getByText('Imported validated progress.', { exact: false })).toBeVisible();
  const after = await page.evaluate(() => JSON.parse(localStorage.getItem('nibli-learn.local.v1')));
  expect(after.progress).toEqual(progress);
  expect(after.session).toEqual(before.session);
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
  await expect(page.getByRole('button', { name: 'Export existing browser data ↓', exact: true })).toBeVisible();
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
  const committed = await page.evaluate(() => JSON.parse(localStorage.getItem('nibli-learn.local.v1')).session);
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
  const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('nibli-learn.local.v1')));
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

import { expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const lesson = '/nibli-learn/#/lesson/found.investigate-release/step/1';
const policy = 'all $build: approves(TestRunner, $build) & approves(Reviewer, $build) -> ready($build, Release).';

const releaseIds = ['release.one-approval','release.different-build','release.same-build','release.execution-and-proof','release.independent-support','release.protected-conclusion','release.repair-policy'];
async function workspace(page, platformBase = "/nibli-learn/") {
  await expect(page.getByRole('button', { name: 'Saved on this device', exact: true })).toBeVisible();
  return page.evaluate(async base => {
    const platform = await import(`${base}platform/platform.js`);
    const result = JSON.parse(await platform.loadWorkspace());
    if (!result.ok || !result.value) throw new Error('Workspace not saved');
    return result.value.data;
  }, platformBase);
}
async function localView(page, platformBase = "/nibli-learn/") {
  const value = await workspace(page, platformBase);
  return { ...value, session: value.projects[value.active_project]?.session, progress: { steps: releaseIds.map(id => value.progress.lessons['found.investigate-release']?.['1']?.[id]?.['1'] ?? {}) } };
}

async function openLesson(page, step = 1, forceReload = false) {
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
  if (forceReload) await page.reload();
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

export async function runReleaseJourney(page, testInfo, { platformBase = '/nibli-learn/', forceReload = false } = {}) {
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await openLesson(page, 1, forceReload);
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
  const proofWorkspace = await workspace(page, platformBase);
  expect(exported.engine_revision).toBe(proofWorkspace.projects[proofWorkspace.active_project].session.engine_revision);
  expect(exported.app_revision).toEqual(expect.any(String));
  expect(exported.content_version).toBe(proofWorkspace.progress.course_version);
  expect(exported.profile).toEqual(exported.run.record.profile ?? null);
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
  await page.getByRole('button', { name: 'Show a hint', exact: true }).click();
  await expect(page.locator('.lesson-panel .hint-box')).toBeVisible();
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
  const progress = (await localView(page, platformBase)).progress;
  expect(progress.steps.every(step => step.checked)).toBe(true);
  expect(errors).toEqual([]);

  return { proof: exported, progress };
}

export { workspace, localView, openLesson, ready, run };

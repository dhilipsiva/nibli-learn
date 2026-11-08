import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { runReleaseJourney } from './helpers/release-journey.mjs';

const course = JSON.parse(await readFile(new URL('../content/course.json', import.meta.url), 'utf8'));
const references = JSON.parse(await readFile(new URL('../content/references.json', import.meta.url), 'utf8'));
const activity = id => course.lessons.flatMap(lesson => lesson.activities.map(item => ({ ...item, lesson: lesson.id }))).find(item => item.id === id);
async function ready(page) {
  await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'ready');
}
async function openActivity(page, id) {
  const item = activity(id);
  await page.goto(`/nibli-learn/#/lesson/${item.lesson}/activity/${item.id}`);
  await ready(page);
  await expect(page.getByRole('heading', { level: 1, name: item.title, exact: true })).toBeVisible();
  return item;
}
async function platform(page, base, method, ...args) {
  return page.evaluate(async ({ base, method, args }) => {
    const api = await import(`${base}platform/platform.js`);
    const response = JSON.parse(await api[method](...args));
    if (!response.ok) throw new Error(`${response.error.kind}: ${response.error.message}`);
    return response.value;
  }, { base, method, args });
}
async function runActivity(page, family, status) {
  await page.getByTestId('activity-run').click();
  await ready(page);
  await expect(page.getByTestId('operation-result')).toHaveAttribute('data-family', family);
  if (status) await expect(page.getByTestId('operation-result').locator('[data-status]')).toHaveAttribute('data-status', status);
}

test('an explicitly installed companion runs F1, U2 and F5 with hints, references and fonts on a fresh offline page', async ({ page, context, baseURL }, testInfo) => {
  test.setTimeout(180_000);
  const origin = new URL(baseURL).origin;
  const externalRequests = [];
  const offlineResponses = [];
  const errors = [];
  let disconnected = false;
  context.on('request', request => {
    const url = new URL(request.url());
    if (['http:', 'https:'].includes(url.protocol) && url.origin !== origin) externalRequests.push(url.href);
  });
  context.on('response', response => {
    if (disconnected) offlineResponses.push({ url: response.url(), status: response.status(), serviceWorker: response.fromServiceWorker() });
  });
  context.on('page', opened => opened.on('pageerror', error => errors.push(error.message)));
  page.on('pageerror', error => errors.push(error.message));
  await context.addInitScript(() => {
    const NativeWorker = window.Worker;
    window.offlineWorkerUrls = [];
    window.offlineWorkerRequests = [];
    window.offlineWorkerResponses = [];
    window.Worker = class extends NativeWorker {
      constructor(url, options) {
        super(url, options);
        window.offlineWorkerUrls.push(new URL(url, location.href).href);
        this.addEventListener('message', event => window.offlineWorkerResponses.push(typeof event.data === 'string' ? JSON.parse(event.data) : event.data));
      }
      postMessage(value, ...rest) {
        window.offlineWorkerRequests.push(typeof value === 'string' ? JSON.parse(value) : value);
        return super.postMessage(value, ...rest);
      }
    };
  });

  await page.goto('/nibli-learn/#/packs');
  await ready(page);
  await expect(page.getByRole('heading', { name: 'Take the investigation with you.', exact: true })).toBeVisible();
  await expect(page.getByText('No complete active offline installation is confirmed.', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Download and verify', exact: true }).click();
  await expect(page.getByText('Download verified. Review the version below, then activate when ready.', { exact: true })).toBeVisible({ timeout: 90_000 });
  const downloaded = await platform(page, '/nibli-learn/', 'offlineStatus');
  expect(downloaded.installed).toBe(false);
  expect(downloaded.active).toBeNull();
  expect(downloaded.installations).toHaveLength(1);
  const installation = downloaded.installations[0];
  expect(installation.complete).toBe(true);
  expect(installation.content_version).toBe(3);
  await page.getByRole('button', { name: 'Activate this verified version', exact: true }).click();
  await expect(page.getByText('Installed and verified for offline use.', { exact: true })).toBeVisible();
  expect((await platform(page, '/nibli-learn/', 'offlineStatus')).active.installation_id).toBe(installation.id);
  await page.evaluate(() => navigator.serviceWorker.ready);

  // A new document must acquire its actual app WASM, worker and fonts while offline.
  // None of the worker, model, result, fetch or service-worker implementations is mocked.
  disconnected = true;
  await context.setOffline(true);
  await page.close();
  const offline = await context.newPage();
  // Reapply after creating the target: browser versions differ in when a new
  // page inherits emulation. The uncached request below is the network gate.
  await context.setOffline(true);
  const first = await openActivity(offline, 'found.inputs-given');
  const networkControl = await offline.evaluate(async () => {
    try {
      const response = await fetch(`/__nibli_offline_network_probe__?nonce=${crypto.randomUUID()}`, { cache: 'no-store', credentials: 'omit' });
      return { blocked: false, status: response.status, reportedOnline: navigator.onLine };
    } catch (error) {
      return { blocked: true, error: error.name, reportedOnline: navigator.onLine };
    }
  });
  expect(networkControl.blocked).toBe(true);
  expect(await offline.evaluate(() => Boolean(navigator.serviceWorker.controller))).toBe(true);
  const workerUrls = await offline.evaluate(() => window.offlineWorkerUrls);
  expect(workerUrls.length).toBeGreaterThan(0);
  expect(workerUrls.every(url => url === `${origin}${installation.runtime_base}worker/bootstrap.js`)).toBe(true);

  await offline.getByLabel('Your prediction or diagnosis').fill('FALSE: the safety approval is not an accepted premise.');
  await runActivity(offline, 'decision', 'FALSE');
  await offline.getByTestId('action-admit-approval').click();
  await ready(offline);
  await expect(offline.getByTestId('operation-result').locator('[data-status]')).toHaveAttribute('data-status', 'TRUE');
  await offline.getByRole('button', { name: 'Reveal next hint', exact: true }).click();
  await expect(offline.getByText(first.hints[0], { exact: true })).toBeVisible();
  await offline.getByRole('button', { name: 'Show the reasoning', exact: true }).click();
  await expect(offline.getByText(first.solution, { exact: true })).toBeVisible();

  await openActivity(offline, 'use.aggregate-results');
  await runActivity(offline, 'aggregated');
  await expect(offline.getByTestId('operation-result')).toContainText('11');
  const records = await offline.evaluate(() => window.offlineWorkerResponses.map(reply => reply.result?.Ok?.operation).filter(Boolean));
  expect(records.some(run => run.result.family === 'decision' && run.result.outcome.status === 'FALSE')).toBe(true);
  expect(records.some(run => run.result.family === 'decision' && run.result.outcome.status === 'TRUE')).toBe(true);
  expect(records.some(run => run.result.family === 'aggregated')).toBe(true);
  expect(records.every(run => run.engine_revision === installation.engine_revision && run.content_version === 3)).toBe(true);
  expect(records.some(run => Array.isArray(run.record.events) && run.record.events.length > 0)).toBe(true);

  await offline.getByRole('button', { name: 'Reference', exact: true }).click();
  await expect(offline.getByRole('heading', { name: 'Keep the meaning in view.', exact: true })).toBeVisible();
  const reference = references.entries.find(entry => entry.id === 'derivable');
  await offline.getByLabel('Search references').fill(reference.title);
  await offline.getByRole('navigation', { name: 'References', exact: true }).getByRole('button', { name: new RegExp(reference.title) }).click();
  await expect(offline.getByText(reference.body, { exact: true })).toBeVisible();
  await offline.getByLabel('KR source').fill('approves(Reviewer, Cart7).');
  await offline.getByRole('button', { name: 'Compile and inspect', exact: true }).click();
  await ready(offline);
  await expect(offline.getByTestId('operation-result')).toHaveAttribute('data-family', 'compiled');

  const release = await runReleaseJourney(offline, testInfo, { platformBase: installation.runtime_base, forceReload: true });
  expect(release.progress.steps.every(step => step.checked)).toBe(true);
  expect(release.proof.run.outcome.status).toBe('TRUE');

  // Integrate's authored review and downloadable local sources must also work
  // from the installed runtime without reaching a model or the network.
  const review = await openActivity(offline, 'integrate.review-triad');
  await runActivity(offline, 'decision', 'FALSE');
  await offline.getByText('Review an application draft', { exact: true }).click();
  await offline.getByRole('button', { name: 'Prepare readings and admission check', exact: true }).click(); await ready(offline);
  await offline.getByRole('button', { name: 'Reject this draft', exact: true }).click();
  await offline.getByLabel('Formal draft for review', { exact: true }).fill(review.review_exercise.repair.formal_text);
  await offline.getByRole('button', { name: 'Prepare readings and admission check', exact: true }).click(); await ready(offline);
  await offline.getByLabel('I reviewed the source, roles, scope and negation; I approve this exact draft for admission.').check();
  await offline.getByRole('button', { name: 'Admit reviewed draft to main project', exact: true }).click(); await ready(offline);
  await offline.getByTestId('action-query-reviewed').click(); await ready(offline);
  await expect(offline.getByTestId('operation-result').locator('[data-status]')).toHaveAttribute('data-status', 'TRUE');
  await openActivity(offline, 'integrate.runtime-fuel-recovery');
  const kitLink = offline.getByRole('link', { name: 'Download pinned local lab kit', exact: true });
  await expect(kitLink).toHaveAttribute('href', `${installation.runtime_base}assets/nibli-labs-v1.tar.gz`);
  const downloadingKit = offline.waitForEvent('download'); await kitLink.click();
  const kitPath = testInfo.outputPath('offline-local-labs.tar.gz'); await (await downloadingKit).saveAs(kitPath);
  expect(await readFile(kitPath)).toEqual(await readFile(new URL('../dist/nibli-learn/assets/nibli-labs-v1.tar.gz', import.meta.url)));
  await expect(offline.locator('.local-lab-panel')).toContainText('wasi-runtime');

  const fonts = await offline.evaluate(async () => {
    const samples = ['400 16px "IBM Plex Sans"', '400 16px "IBM Plex Mono"', '600 16px "Space Grotesk"'];
    return Promise.all(samples.map(async font => ({ font, faces: (await document.fonts.load(font, 'Offline learning 42')).map(face => face.status), available: document.fonts.check(font, 'Offline learning 42') })));
  });
  expect(fonts.every(item => item.available && item.faces.length > 0 && item.faces.every(status => status === 'loaded'))).toBe(true);
  await expect(offline.getByRole('button', { name: 'Saved on this device', exact: true })).toBeVisible();
  const saved = await platform(offline, installation.runtime_base, 'loadWorkspace');
  expect(saved.data.schema).toBe(2);
  expect(saved.data.progress.lessons[first.lesson]['1'][first.id]['1'].hints_revealed).toBe(1);
  expect((await platform(offline, installation.runtime_base, 'offlineStatus')).installed).toBe(true);
  const runtimeResponses = offlineResponses.filter(response => response.url.startsWith(`${origin}${installation.runtime_base}`));
  expect(runtimeResponses.some(response => response.url.endsWith('.wasm') && response.serviceWorker && response.status === 200)).toBe(true);
  expect(runtimeResponses.some(response => response.url.endsWith('.woff2') && response.serviceWorker && response.status === 200)).toBe(true);
  expect(runtimeResponses.filter(response => response.status >= 400)).toEqual([]);
  expect(externalRequests).toEqual([]);
  expect(errors).toEqual([]);
  await offline.screenshot({ path: testInfo.outputPath('offline-complete-investigation.png'), fullPage: true });
  await testInfo.attach('offline-installation-evidence', { body: JSON.stringify({ installation, networkControl, operations: records.map(({ id, engine_revision, content_version, result }) => ({ id, engine_revision, content_version, result })), fonts, runtimeResponses, externalRequests }, null, 2), contentType: 'application/json' });
});

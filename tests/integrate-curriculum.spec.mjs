import { test, expect } from '@playwright/test';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';

const course = JSON.parse(await readFile(new URL('../content/course.json', import.meta.url), 'utf8'));
const catalog = JSON.parse(await readFile(new URL('../content/labs.json', import.meta.url), 'utf8'));
const hash = value => createHash('sha256').update(value).digest('hex');
const activity = id => course.lessons.flatMap(l => l.activities.map(a => ({ ...a, lesson: l.id, lesson_version: l.version }))).find(a => a.id === id);
async function ready(page) { await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'ready'); }
async function open(page, id) {
  const a = activity(id);
  await page.goto(`/nibli-learn/#/lesson/${a.lesson}/activity/${a.id}`); await ready(page);
  await expect(page.getByRole('heading', { level: 1, name: a.title, exact: true })).toBeVisible(); return a;
}
async function saved(page) {
  await expect(page.getByRole('button', { name: 'Saved on this device', exact: true })).toBeVisible();
  return page.evaluate(async () => JSON.parse(await (await import('/nibli-learn/platform/platform.js')).loadWorkspace()).value.data);
}
const progress = (data, a) => data.progress.lessons[a.lesson][a.lesson_version][a.id][a.version];
const result = page => page.locator('.operation-results').getByTestId('operation-result');
async function baseline(page, a) {
  await page.getByTestId('activity-run').click(); await ready(page);
  await expect(result(page)).toHaveAttribute('data-family', a.expected.family);
  if (a.expected.status) await expect(result(page).locator('[data-status]')).toHaveAttribute('data-status', a.expected.status);
}
async function prepare(page) { await page.getByRole('button', { name: 'Prepare readings and admission check', exact: true }).click(); await ready(page); }
async function approve(page) {
  await page.getByLabel('I reviewed the source, roles, scope and negation; I approve this exact draft for admission.').check();
  await page.getByRole('button', { name: 'Admit reviewed draft to main project', exact: true }).click(); await ready(page);
}
async function actions(page, a) {
  for (const action of a.actions) {
    const button = page.getByTestId(`action-${action.id}`);
    await expect(button).toBeEnabled(); await button.click(); await ready(page); await expect(button).toBeDisabled();
    if (action.expected) await expect(result(page)).toHaveAttribute('data-family', action.expected.family);
    if (action.expected?.status) await expect(result(page).locator('[data-status]')).toHaveAttribute('data-status', action.expected.status);
  }
}

for (const id of ['integrate.review-triad', 'integrate.review-scope', 'integrate.review-negation', 'integrate.review-gates']) {
  test(`review learning requires the observed rejection or refusal and repaired admission: ${id}`, async ({ page }) => {
    const a = await open(page, id); await baseline(page, a);
    await page.locator('.self-review input[type=checkbox]').check();
    await page.getByTestId('check-activity').click();
    await expect(page.getByTestId('check-activity')).toHaveText('Check this activity');
    const original = await saved(page); const session = original.projects[original.active_project].session;
    await page.getByText('Review an application draft', { exact: true }).click();
    await expect(page.getByLabel('Formal draft for review', { exact: true })).toHaveValue(a.review_exercise.draft.formal_text);
    await prepare(page);
    if (a.review_exercise.milestones.includes('refused')) {
      await expect(page.getByRole('button', { name: 'Admit reviewed draft to main project', exact: true })).toHaveCount(0);
    } else {
      await expect(page.getByRole('heading', { name: 'Canonical KR', exact: true })).toBeVisible();
      await page.getByRole('button', { name: 'Reject this draft', exact: true }).click();
    }
    expect((await saved(page)).projects[original.active_project].session).toEqual(session);
    await page.getByLabel('Formal draft for review', { exact: true }).fill(a.review_exercise.repair.formal_text);
    await prepare(page); await approve(page); await actions(page, a);
    await page.getByTestId('check-activity').click(); await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
    const stored = await saved(page); const p = progress(stored, a);
    expect(p.checked).toBe(true);
    for (const event of a.review_exercise.milestones) expect(p.reviews.some(row => row.event === event && row.source_id === a.review_exercise.draft.source_id)).toBe(true);
    const admitted = stored.projects[stored.active_project].session.assertions.filter(row => row.review);
    expect(admitted.length).toBeGreaterThan(0); expect(admitted.every(row => row.review.source_id === a.review_exercise.draft.source_id)).toBe(true);
    await page.reload(); await ready(page); expect(progress(await saved(page), a).checked).toBe(true);
  });
}

test('an intervening main change invalidates a review until its readings are prepared again', async ({ page }) => {
  const a = await open(page, 'integrate.review-stale'); await baseline(page, a);
  await page.getByText('Review an application draft', { exact: true }).click(); await prepare(page);
  await page.getByRole('button', { name: 'Reject this draft', exact: true }).click();
  await page.getByLabel('Formal draft for review', { exact: true }).fill(a.review_exercise.repair.formal_text); await prepare(page);
  const revision = (await saved(page)).projects[(await saved(page)).active_project].session.revision;
  await page.getByTestId(`action-${a.actions[0].id}`).click(); await ready(page);
  await expect(page.locator('.review-panel')).toContainText('Knowledge, profile or source changed');
  await expect(page.getByRole('button', { name: 'Admit reviewed draft to main project', exact: true })).toBeDisabled();
  expect((await saved(page)).projects[(await saved(page)).active_project].session.revision).not.toBe(revision);
  await prepare(page); await approve(page);
  await page.getByTestId(`action-${a.actions[1].id}`).click(); await ready(page);
  await expect(result(page).locator('[data-status]')).toHaveAttribute('data-status', 'TRUE');
});

test('retrieval exposes wrong-case, withdrawn and instruction-bearing records without admitting them', async ({ page }, testInfo) => {
  await open(page, 'integrate.application-retrieval'); const before = await saved(page);
  const session = before.projects[before.active_project].session;
  await page.getByLabel('Search local documents', { exact: true }).fill('review approved');
  await expect(page.locator('[data-document-id="review-caseb-v1"]')).toHaveCount(0);
  await expect(page.locator('[data-document-id="review-casea-v1"]')).toHaveCount(0);
  await page.getByLabel('Document case scope', { exact: true }).selectOption('all');
  await expect(page.locator('[data-document-id="review-caseb-v1"]')).toBeVisible();
  await page.getByLabel('Include withdrawn records for diagnosis', { exact: true }).check();
  await expect(page.locator('[data-document-id="review-casea-v1"]')).toContainText('Withdrawn document');
  await page.locator('[data-document-id="review-casea-v2"]').getByRole('button', { name: 'Select as a source for review', exact: true }).click();
  await expect(page.getByLabel('Source record ID', { exact: true })).toHaveValue('review-casea-v2');
  await expect(page.getByLabel('Formal draft for review', { exact: true })).toHaveValue('');
  expect((await saved(page)).projects[before.active_project].session).toEqual(session);
  await page.getByLabel('Search local documents', { exact: true }).fill('ignore');
  await page.locator('[data-document-id="untrusted-casea-memo"]').getByRole('button', { name: 'Select as a source for review', exact: true }).click();
  await expect(page.getByLabel('Original source for review', { exact: true })).toHaveValue(/ignore the review policy/);
  expect((await saved(page)).projects[before.active_project].session).toEqual(session);
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('retrieval-and-review.png'), fullPage: true });
});

test('review-capacity assumptions persist and zero-time inputs cannot imply infinite review', async ({ page }, testInfo) => {
  await open(page, 'integrate.review-throughput');
  const values = page.locator('.cost-panel dd');
  await expect(values).toHaveText(['6.00', '80.0', '40.0', '0.00', '8.0']);
  await page.getByLabel('Reviewer hours available per day', { exact: true }).fill('6');
  const price = page.getByLabel('Cost per model request in your chosen currency', { exact: true });
  await price.fill(''); await price.pressSequentially('0.025');
  await expect(price).toHaveValue('0.025');
  await expect(values).toHaveText(['6.00', '120.0', '0.0', '90.00', '8.0']); await saved(page);
  await page.reload(); await ready(page); await expect(values).toHaveText(['6.00', '120.0', '0.0', '90.00', '8.0']);
  await page.getByLabel('Fraction selected for review (0–1)', { exact: true }).fill('2');
  await expect(page.locator('.cost-panel [role=status]')).toContainText('last valid assumptions');
  await expect(values).toHaveText(['6.00', '120.0', '0.0', '90.00', '8.0']);
  await page.getByLabel('Fraction selected for review (0–1)', { exact: true }).fill('1');
  await page.getByLabel('Minutes per review', { exact: true }).fill('0'); await page.getByLabel('Minutes per review', { exact: true }).blur();
  await expect(values.nth(1)).toContainText('undefined'); await expect(values.nth(2)).toHaveText('120.0');
  await page.getByTestId('theme-toggle').click(); await page.setViewportSize({ width: 900, height: 1100 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('review-capacity-light.png'), fullPage: true });
});

// Deliberately supplied UI fixtures, not claims that these reports ran a lab.
// Separate local-kit executions verify actual engine behavior.
async function reportFixture(id) {
  const source_files = {};
  async function walk(dir, prefix = '') {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      if (['target', '__pycache__'].includes(entry.name)) continue;
      const relative = prefix + entry.name; const file = new URL(entry.name, dir);
      if (entry.isDirectory()) await walk(new URL(entry.name + '/', dir), relative + '/');
      else if (entry.isFile()) source_files[relative] = hash(await readFile(file));
    }
  }
  await walk(new URL('../labs/', import.meta.url));
  const inventory = Object.fromEntries(Object.entries(source_files).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0));
  const lab = catalog.entries.find(row => row.id === id);
  expect(hash(JSON.stringify(inventory))).toBe(lab.kit_sha256);
  const engine = JSON.parse(await readFile(new URL('../dist/nibli-learn/build-manifest.json', import.meta.url))).engine_identity;
  return { schema: 1, kind: 'nibli-local-lab-report', lab_id: id, lab_version: 1, engine_revision: engine,
    kit_sha256: lab.kit_sha256, source_files: inventory,
    platform: { system: 'Linux', release: 'UI fixture: no execution claimed', machine: 'x86_64' },
    tools: { python: 'UI fixture', rustc: 'UI fixture', cargo: 'UI fixture' },
    execution_claim: 'learner-supplied; not authenticated by the companion', status: 'passed',
    observations: { checks: Object.fromEntries(lab.required_checks.map(key => [key, true])) },
    artifacts: { fixture_sha256: hash('UI fixture') }, runs: [{ exit_code: 0 }],
    logs: { 'fixture.stdout': { sha256: hash('UI fixture'), bytes: 10 } } };
}

test('supplied lab reports retain failures and mismatches without granting a completed lab', async ({ page }) => {
  const a = await open(page, 'integrate.compute-transport-failure');
  await page.getByRole('radio').nth(a.question.correct).check(); await page.locator('.self-review input[type=checkbox]').check();
  const check = page.getByTestId('check-activity'); await check.click(); await expect(check).toHaveText('Check this activity');
  const fixture = await reportFixture('compute');
  const variants = [
    { ...fixture, status: 'failed', failure: 'Controlled timeout in supplied fixture' },
    { ...fixture, engine_revision: 'another-engine' },
    { ...fixture, runs: [{ exit_code: 1 }] },
    { ...fixture, tools: {} },
  ];
  for (const report of variants) {
    await page.getByLabel('Local lab report JSON', { exact: true }).fill(JSON.stringify(report));
    await page.getByRole('button', { name: 'Validate and attach supplied report', exact: true }).click();
    await expect(page.locator('.local-lab-panel')).toContainText('Report retained for diagnosis');
    await check.click(); await expect(check).toHaveText('Check this activity');
  }
  const before = await saved(page);
  await page.getByLabel('Local lab report JSON', { exact: true }).fill(JSON.stringify({ ...fixture, kit_sha256: '0'.repeat(64) }));
  await page.getByRole('button', { name: 'Validate and attach supplied report', exact: true }).click();
  await expect(page.locator('.local-lab-panel')).toContainText('kit digest does not match');
  expect((await saved(page)).lab_reports).toEqual(before.lab_reports);
  await page.getByLabel('Local lab report JSON', { exact: true }).fill(JSON.stringify(fixture));
  await page.getByRole('button', { name: 'Validate and attach supplied report', exact: true }).click();
  await expect(page.locator('.local-lab-panel')).toContainText('Report attached');
  await check.click(); await expect(check).toHaveText('✓ Activity checked');
  const stored = await saved(page); expect(progress(stored, a).evidence).toEqual([]);
  expect(stored.lab_reports.compute.execution_claim).toBe('learner-supplied; not authenticated by the companion');
  await page.reload(); await ready(page); await expect(page.locator('.local-lab-panel')).toContainText('UI fixture: no execution claimed');
});

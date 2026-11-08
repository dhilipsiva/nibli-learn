import { test, expect } from '@playwright/test';
import { readFile, readdir, stat } from 'node:fs/promises';
const course = JSON.parse(await readFile(new URL('../content/course.json', import.meta.url), 'utf8'));
const catalog = JSON.parse(await readFile(new URL('../content/labs.json', import.meta.url), 'utf8'));
const activity = id => course.lessons.flatMap(l => l.activities.map(a => ({ ...a, lesson: l.id }))).find(a => a.id === id);
async function ready(page) { await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'ready'); }
async function open(page, id) {
  const a = activity(id); await page.goto(`/nibli-learn/#/lesson/${a.lesson}/activity/${a.id}`); await ready(page);
  await expect(page.getByRole('heading', { level: 1, name: a.title, exact: true })).toBeVisible(); return a;
}
const result = page => page.locator('.operation-results').getByTestId('operation-result');
async function observed(page, expected) {
  await ready(page); await expect(result(page)).toHaveAttribute('data-family', expected.family);
  if (expected.family === 'decision') await expect(result(page).locator('[data-status]')).toHaveAttribute('data-status', expected.status);
}
async function initial(page, a) { await page.getByTestId('activity-run').click(); await observed(page, a.expected); }
async function actions(page, a) {
  for (const action of a.actions) {
    const button = page.getByTestId(`action-${action.id}`); await expect(button).toBeEnabled(); await button.click();
    await observed(page, action.expected); await expect(button).toBeDisabled();
  }
}
async function capture(page) {
  await page.addInitScript(() => {
    const Original = window.Worker; window.buildRequests = []; window.buildReplies = [];
    window.Worker = class extends Original {
      constructor(...args) { super(...args); this.addEventListener('message', e => window.buildReplies.push(typeof e.data === 'string' ? JSON.parse(e.data) : e.data)); }
      postMessage(v, ...args) { window.buildRequests.push(typeof v === 'string' ? JSON.parse(v) : v); return super.postMessage(v, ...args); }
    };
  });
}
async function latest(page) { return page.evaluate(() => window.buildReplies.map(r => r.result?.Ok?.operation).filter(Boolean).at(-1)); }

test('compiler navigation follows captured spans and nodes without executing another operation', async ({ page }, testInfo) => {
  await capture(page); const a = await open(page, 'build.source-spans'); await initial(page, a);
  const run = await latest(page); const artifact = run.record.artifacts[0];
  expect(artifact.statement_spans).toHaveLength(2);
  await expect(page.getByTestId('compiler-source-fragment')).toHaveText('knows(Emi, "inspection.v2").');
  const requests = await page.evaluate(() => window.buildRequests.length);
  await page.getByRole('button', { name: /^Statement 2/ }).focus(); await page.keyboard.press('Enter');
  await expect(page.getByTestId('compiler-source-fragment')).toHaveText('computer(Console4).');
  await page.getByLabel('Compiler stage', { exact: true }).selectOption('parsed');
  await expect(page.locator('.compiler-inspector')).toContainText('QuotedLiteral');
  await page.getByLabel('Compiler stage', { exact: true }).selectOption('lowered');
  const root = artifact.lowered_logic.roots[0]; await page.getByRole('button', { name: `Node ${root}`, exact: true }).click();
  await expect(page.getByTestId('compiler-node-value')).toHaveText(JSON.stringify(artifact.lowered_logic.nodes[root], null, 2));
  expect(await page.evaluate(() => window.buildRequests.length)).toBe(requests);
  await page.screenshot({ path: testInfo.outputPath('build-compiler-dark.png'), fullPage: true });
  await page.getByTestId('theme-toggle').click(); await page.setViewportSize({ width: 900, height: 1100 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('build-compiler-light-tablet.png'), fullPage: true });
});

test('CountNode follows its body index and XOR exposes actual shared graph parents', async ({ page }) => {
  await capture(page); let a = await open(page, 'build.count-edge'); await initial(page, a);
  let buffer = (await latest(page)).record.artifacts[0].lowered_logic;
  const root = buffer.roots[0]; const count = buffer.nodes[root].CountNode;
  expect(count[1]).toBe(2); await page.getByLabel('Compiler stage', { exact: true }).selectOption('lowered');
  await page.getByRole('button', { name: `Node ${root}`, exact: true }).click();
  await expect(page.getByRole('button', { name: `Child ${count[2]}`, exact: true })).toBeVisible();
  expect(await page.getByRole('button', { name: /^Child / }).count()).toBe(1);
  await page.getByRole('button', { name: `Child ${count[2]}`, exact: true }).click();
  await expect(page.getByTestId('compiler-node-value')).toHaveText(JSON.stringify(buffer.nodes[count[2]], null, 2));
  a = await open(page, 'build.connective-graph'); await initial(page, a);
  buffer = (await latest(page)).record.artifacts[0].lowered_logic;
  // Find an actual repeated binary operand in the captured XOR expansion.
  const edges = buffer.nodes.flatMap(n => n.AndNode || n.OrNode || []);
  const shared = edges.find(n => edges.filter(v => v === n).length > 1); expect(shared).toBeDefined();
  await page.getByLabel('Compiler stage', { exact: true }).selectOption('lowered');
  await page.getByLabel('Inspect node', { exact: true }).fill(String(shared));
  await expect(page.locator('.compiler-inspector')).toContainText('reference this same node index');
});

for (const id of ['build.grammar-validation', 'build.relative-repair', 'build.depth-passes', 'build.support-rebuild', 'build.independent-witnesses', 'build.wrong-model']) {
  test(`Build diagnosis and repair use actual worker outcomes: ${id}`, async ({ page }) => {
    const a = await open(page, id); await initial(page, a); await actions(page, a);
  });
}

test('search and memo filters select recorded events and proof inspection preserves its citations', async ({ page }) => {
  await capture(page); const a = await open(page, 'build.proof-dag'); await initial(page, a);
  const run = await latest(page); expect(run.result.outcome.status).toBe('TRUE');
  const kinds = run.record.events.map(e => e.kind); expect(kinds).toContain('rule_fire'); expect(kinds).toContain('cache_hit');
  const requests = await page.evaluate(() => window.buildRequests.length);
  await page.getByRole('tab', { name: 'Execution replay', exact: true }).click();
  await page.getByLabel('Show', { exact: true }).selectOption('search'); await expect(page.locator('.events .event').first()).toBeVisible();
  await page.getByLabel('Show', { exact: true }).selectOption('memo');
  expect(await page.locator('.events .event-kind').allTextContents()).toEqual(expect.arrayContaining(['cache hit']));
  await page.getByRole('tab', { name: 'Proof', exact: true }).click(); await expect(page.getByTestId('proof-graph')).toBeVisible();
  await page.getByRole('button', { name: 'Text', exact: true }).click(); await expect(page.getByTestId('proof-text')).toContainText('Nori');
  expect(await page.evaluate(() => window.buildRequests.length)).toBe(requests);
});

async function currentNativeReport(id) {
  const base = new URL('../target/', import.meta.url);
  const names = (await readdir(base)).filter(name => name.startsWith('native-lab-check-'));
  const directories = await Promise.all(names.map(async name => ({ name, time: (await stat(new URL(name, base))).mtimeMs })));
  directories.sort((a, b) => b.time - a.time);
  const definition = catalog.entries.find(l => l.id === id);
  for (const { name } of directories) {
    try {
      const value = JSON.parse(await readFile(new URL(`${name}/${id}/report.json`, base), 'utf8'));
      if (value.status === 'passed' && value.kit_sha256 === definition.kit_sha256) return value;
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  throw new Error('Run just ci-native first: this browser journey imports its actual matching compiler lab report.');
}

test('an actual matching native report remains supplied evidence and grants no browser proof', async ({ page }) => {
  const report = await currentNativeReport('compiler-internals'); const a = await open(page, 'build.compiler-lab');
  await page.getByLabel('Local lab report JSON', { exact: true }).fill(JSON.stringify(report));
  await page.getByRole('button', { name: 'Validate and attach supplied report', exact: true }).click();
  await expect(page.locator('.local-lab-panel')).toContainText('Report attached');
  await page.getByRole('radio').nth(a.question.correct).check(); await page.locator('.self-review input[type=checkbox]').check();
  await page.getByTestId('check-activity').click(); await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
  await page.getByRole('tab', { name: 'Proof', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'This operation has no proof certificate', exact: true })).toBeVisible();
  await page.reload(); await ready(page); await expect(page.locator('.local-lab-panel')).toContainText(report.kit_sha256);
});

import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const lesson = '/nibli-learn/#/lesson/found.prediction-derivation/activity/found.inputs-given';
async function ready(page) {
  await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'ready');
}
async function envelope(page, waitForSave = true) {
  if (waitForSave) await expect(page.getByRole('button', { name: 'Saved on this device', exact: true })).toBeVisible();
  return page.evaluate(async () => {
    const api = await import('/nibli-learn/platform/platform.js');
    const result = JSON.parse(await api.loadWorkspace());
    if (!result.ok || !result.value) throw new Error(result.error?.message || 'Workspace has not been committed');
    return result.value;
  });
}
async function dataPage(page) {
  await page.getByRole('link', { name: 'Local data · no account · no analytics', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Your projects, your evidence.', exact: true })).toBeVisible();
}
async function start(page) {
  await page.goto(lesson);
  await ready(page);
  await envelope(page);
  await dataPage(page);
}
async function downloadJson(page, button, testInfo, filename) {
  const downloading = page.waitForEvent('download');
  await button.click();
  const output = testInfo.outputPath(filename);
  await (await downloading).saveAs(output);
  return JSON.parse(await readFile(output, 'utf8'));
}
async function rename(page, previous, next) {
  const names = page.getByLabel('Project name', { exact: true });
  for (const input of await names.all()) {
    if (await input.inputValue() === previous) {
      await input.fill(next);
      await input.press('Tab');
      await expect(input).toHaveValue(next);
      return;
    }
  }
  throw new Error(`Project name input not found: ${previous}`);
}
async function openSavedProject(page, name) {
  for (const row of await page.locator('.project-row').all()) {
    if (await row.getByLabel('Project name', { exact: true }).inputValue() === name) {
      await row.getByRole('button', { name: 'Open', exact: true }).click();
      await ready(page);
      return;
    }
  }
  throw new Error(`Saved project not found: ${name}`);
}
async function preview(page, kind, value) {
  await page.getByLabel('Import type').selectOption(kind);
  await page.getByLabel('Exported JSON').fill(JSON.stringify(value));
  await page.getByRole('button', { name: 'Validate and preview', exact: true }).click();
  await ready(page);
}

test('project preview stays detached, apply renews identity and provenance, and undo restores the prior workspace', async ({ page }, testInfo) => {
  await start(page);
  const before = await envelope(page);
  const source = await downloadJson(page, page.getByRole('button', { name: 'Export project', exact: true }).first(), testInfo, 'source-project.json');
  expect(source.schema).toBe(2);
  expect(source.id).toBe(before.data.active_project);
  const candidate = { ...source, name: 'Imported copy review' };
  await preview(page, 'project', candidate);
  await expect(page.getByRole('heading', { name: 'Validated preview', exact: true })).toBeVisible();
  const detached = await envelope(page);
  expect(detached.revision).toBe(before.revision);
  expect(detached.data).toEqual(before.data);
  await page.getByText('Validated project and profile', { exact: true }).click();
  const previewed = JSON.parse(await page.locator('.import-preview pre').textContent());
  expect(previewed.id).not.toBe(source.id);
  expect(previewed.session.epoch).not.toBe(source.session.epoch);
  expect(previewed.session.revision).toBe('0');
  expect(previewed.imported_from).toEqual({ project_id: source.id, epoch: source.session.epoch, revision: source.session.revision, engine_revision: source.engine_revision });

  await page.getByRole('button', { name: 'Apply this validated import', exact: true }).click();
  await ready(page);
  const applied = await envelope(page);
  expect(Object.keys(applied.data.projects)).toHaveLength(Object.keys(before.data.projects).length + 1);
  expect(applied.data.active_project).toBe(previewed.id);
  expect(applied.data.projects[source.id]).toEqual(before.data.projects[source.id]);
  expect(applied.data.projects[previewed.id].session.epoch).toBe(previewed.session.epoch);
  expect(applied.data.projects[previewed.id].imported_from).toEqual(previewed.imported_from);
  expect(applied.can_undo).toBe(true);
  const importedLessonUrl = page.url();
  await dataPage(page);
  await rename(page, candidate.name, 'Ordinary edit after import');
  await expect.poll(async () => (await envelope(page, false)).data.projects[previewed.id].name).toBe('Ordinary edit after import');
  const renamed = (await envelope(page)).data.projects[previewed.id];
  const canonicalId = `${renamed.lesson}-${renamed.activity}`;
  expect(renamed.id).not.toBe(canonicalId);
  // Reload the imported project's saved lesson URL. Replacing only the address
  // avoids turning this boot test into an explicit selection of a fresh activity.
  await page.evaluate(url => history.replaceState(null, '', url), importedLessonUrl);
  await page.reload();
  await ready(page);
  const reloadedImport = await envelope(page);
  expect(reloadedImport.data.active_project).toBe(previewed.id);
  expect(reloadedImport.data.projects[previewed.id].session.epoch).toBe(renamed.session.epoch);
  expect(reloadedImport.data.projects[previewed.id].session.revision).toBe(renamed.session.revision);
  expect(reloadedImport.data.projects[previewed.id].name).toBe(renamed.name);
  // Opening another saved project and returning through the imported row must
  // preserve the same identity on the next real reload as well.
  await dataPage(page);
  await openSavedProject(page, source.name);
  await expect.poll(async () => (await envelope(page, false)).data.active_project).toBe(source.id);
  await dataPage(page);
  await openSavedProject(page, renamed.name);
  await expect.poll(async () => (await envelope(page, false)).data.active_project).toBe(previewed.id);
  await page.reload();
  await ready(page);
  const reopened = await envelope(page);
  expect(reopened.data.active_project).toBe(previewed.id);
  expect(reopened.data.projects[previewed.id].session.epoch).toBe(renamed.session.epoch);
  expect(reopened.data.projects[previewed.id].session.revision).toBe(renamed.session.revision);
  expect(reopened.data.projects[previewed.id].name).toBe(renamed.name);
  await dataPage(page);
  await page.getByRole('button', { name: 'Undo last replacement', exact: true }).click();
  await ready(page);
  const undone = await envelope(page);
  expect(undone.data.projects).toEqual(before.data.projects);
  expect(undone.data.progress).toEqual(before.data.progress);
  expect(undone.data.active_project).toBe(before.data.active_project);
  expect(undone.can_undo).toBe(false);
  expect(BigInt(undone.revision)).toBeGreaterThan(BigInt(applied.revision));
});

test('invalid and future whole-workspace imports preserve the accepted workspace and never offer apply', async ({ page }, testInfo) => {
  await start(page);
  const exported = await downloadJson(page, page.getByRole('button', { name: 'Export all local work', exact: true }), testInfo, 'workspace-before-invalid-imports.json');
  const before = await envelope(page);
  const invalid = { ...exported, active_project: 'missing-project' };
  const future = structuredClone(exported); future.progress.course_version += 1;
  for (const candidate of [invalid, future, { ...exported, schema: 999 }]) {
    await preview(page, 'workspace', candidate);
    await expect(page.getByRole('alert')).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Validated preview', exact: true })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Apply this validated import', exact: true })).toHaveCount(0);
    const retained = await envelope(page);
    expect(retained.revision).toBe(before.revision);
    expect(retained.data).toEqual(before.data);
  }
});

test('two tabs retain a conflicting local edit until an explicit reload of the committed workspace', async ({ page, context }, testInfo) => {
  await start(page);
  const initial = await envelope(page);
  const id = initial.data.active_project;
  const originalName = initial.data.projects[id].name;
  const writer = await context.newPage();
  await writer.goto('/nibli-learn/#/data');
  await ready(writer);
  await envelope(writer);
  await rename(writer, originalName, 'Committed in the second tab');
  const committed = await envelope(writer);
  expect(committed.data.projects[id].name).toBe('Committed in the second tab');
  await expect(page.getByText('Another tab has newer saved work. Current unsaved changes have not been overwritten.', { exact: true })).toBeVisible();
  // A stale tab remains editable/exportable, but its candidate cannot replace storage.
  await rename(page, originalName, 'Unsaved first-tab candidate');
  const stillCommitted = await envelope(page, false);
  expect(stillCommitted.revision).toBe(committed.revision);
  expect(stillCommitted.data.projects[id].name).toBe('Committed in the second tab');
  const unsaved = await downloadJson(page, page.getByRole('button', { name: 'Export this tab’s copy', exact: true }), testInfo, 'retained-conflicting-candidate.json');
  expect(unsaved.projects[id].name).toBe('Unsaved first-tab candidate');
  await page.getByRole('button', { name: 'Reload saved workspace', exact: true }).click();
  await ready(page);
  await expect(page.getByRole('button', { name: 'Reload saved workspace', exact: true })).toHaveCount(0);
  await expect(page.getByLabel('Project name', { exact: true })).toHaveValue('Committed in the second tab');
  const reloaded = await envelope(page);
  expect(reloaded.data.projects[id].name).toBe('Committed in the second tab');
  await writer.close();
});

test('a controlled browser quota failure keeps durable work intact and unsaved UI work exportable', async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    window.failWorkspaceWrites = false;
    const original = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function(value, key) {
      if (window.failWorkspaceWrites && key === 'workspace') throw new DOMException('Controlled test quota exhaustion', 'QuotaExceededError');
      return original.call(this, value, key);
    };
  });
  await start(page);
  const before = await envelope(page);
  const id = before.data.active_project;
  await page.evaluate(() => { window.failWorkspaceWrites = true; });
  await rename(page, before.data.projects[id].name, 'Exportable despite quota');
  await expect(page.getByRole('alert')).toContainText('Not saved (quota)');
  await expect(page.getByRole('button', { name: 'Saved on this device', exact: true })).toHaveCount(0);
  const durable = await envelope(page, false);
  expect(durable.revision).toBe(before.revision);
  expect(durable.data).toEqual(before.data);
  const unsaved = await downloadJson(page, page.getByRole('button', { name: 'Export all local work', exact: true }), testInfo, 'quota-unsaved-workspace.json');
  expect(unsaved.projects[id].name).toBe('Exportable despite quota');
  const raw = await downloadJson(page, page.getByRole('button', { name: 'Export retained raw data', exact: true }), testInfo, 'quota-committed-recovery.json');
  expect(raw.workspace.data).toEqual(before.data);
  expect(raw.workspace.revision).toBe(before.revision);
});

test('whole-workspace preview is detached and apply gives every imported project a fresh identity with provenance', async ({ page }, testInfo) => {
  await start(page);
  await page.getByRole('button', { name: 'New sandbox project', exact: true }).click();
  await ready(page);
  await envelope(page);
  await dataPage(page);
  const before = await envelope(page);
  expect(Object.keys(before.data.projects)).toHaveLength(2);
  const source = await downloadJson(page, page.getByRole('button', { name: 'Export all local work', exact: true }), testInfo, 'workspace-source-for-import.json');
  await preview(page, 'workspace', source);
  await expect(page.getByRole('heading', { name: 'Validated preview', exact: true })).toBeVisible();
  await expect(page.locator('.import-preview')).toContainText('2 independently validated projects');
  const detached = await envelope(page);
  expect(detached.revision).toBe(before.revision);
  expect(detached.data).toEqual(before.data);
  await page.getByRole('button', { name: 'Apply this validated import', exact: true }).click();
  await ready(page);
  await expect.poll(async () => (await envelope(page, false)).can_undo).toBe(true);
  const applied = await envelope(page);
  expect(Object.keys(applied.data.projects)).toHaveLength(2);
  expect(applied.data.progress).toEqual(source.progress);
  expect(Object.keys(applied.data.projects).some(id => id in source.projects)).toBe(false);
  const epochs = new Set();
  for (const project of Object.values(applied.data.projects)) {
    const previous = source.projects[project.imported_from.project_id];
    expect(previous).toBeTruthy();
    expect(project.id).not.toBe(previous.id);
    expect(project.session.epoch).not.toBe(previous.session.epoch);
    expect(project.session.revision).toBe('0');
    expect(project.imported_from).toEqual({ project_id: previous.id, epoch: previous.session.epoch, revision: previous.session.revision, engine_revision: previous.engine_revision });
    expect(project.session.assertions).toEqual(previous.session.assertions);
    expect(project.session.profile).toEqual(previous.session.profile);
    epochs.add(project.session.epoch);
  }
  expect(epochs.size).toBe(2);
  expect(applied.data.projects[applied.data.active_project].imported_from.project_id).toBe(source.active_project);
  await dataPage(page);
  await page.getByRole('button', { name: 'Undo last replacement', exact: true }).click();
  await ready(page);
  const undone = await envelope(page);
  expect(undone.data.projects).toEqual(before.data.projects);
  expect(undone.data.active_project).toBe(before.data.active_project);
});

test('a failed import save retains its checkpoint intent for an ordinary retry and a real undo', async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    window.failImportSave = false;
    const original = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function(value, key) {
      if (window.failImportSave && key === 'workspace') throw new DOMException('Controlled import save quota failure', 'QuotaExceededError');
      return original.call(this, value, key);
    };
  });
  await start(page);
  const before = await envelope(page);
  const source = await downloadJson(page, page.getByRole('button', { name: 'Export project', exact: true }).first(), testInfo, 'quota-import-source.json');
  const candidate = { ...source, name: 'Imported while quota failed' };
  await preview(page, 'project', candidate);
  await expect(page.getByRole('heading', { name: 'Validated preview', exact: true })).toBeVisible();
  await page.evaluate(() => { window.failImportSave = true; });
  await page.getByRole('button', { name: 'Apply this validated import', exact: true }).click();
  await ready(page);
  await dataPage(page);
  await expect(page.getByRole('alert')).toContainText('Not saved (quota)');
  const retained = await envelope(page, false);
  expect(retained.revision).toBe(before.revision);
  expect(retained.data).toEqual(before.data);
  expect(retained.can_undo).toBe(false);
  const unsaved = await downloadJson(page, page.getByRole('button', { name: 'Export all local work', exact: true }), testInfo, 'quota-import-unsaved.json');
  const importedId = unsaved.active_project;
  expect(importedId).not.toBe(source.id);
  expect(unsaved.projects[importedId].name).toBe(candidate.name);
  expect(unsaved.projects[importedId].imported_from.project_id).toBe(source.id);
  await page.evaluate(() => { window.failImportSave = false; });
  // An ordinary user edit retries the pending durable save; it is not another import.
  await rename(page, candidate.name, 'Imported after storage recovered');
  await expect.poll(async () => (await envelope(page, false)).data.projects[importedId]?.name).toBe('Imported after storage recovered');
  const retried = await envelope(page);
  expect(retried.can_undo).toBe(true);
  const raw = await downloadJson(page, page.getByRole('button', { name: 'Export retained raw data', exact: true }), testInfo, 'quota-import-restored-checkpoint.json');
  expect(raw.backup.data).toEqual(before.data);
  await page.getByRole('button', { name: 'Undo last replacement', exact: true }).click();
  await ready(page);
  const undone = await envelope(page);
  expect(undone.data.projects).toEqual(before.data.projects);
  expect(undone.data.progress).toEqual(before.data.progress);
  expect(undone.data.active_project).toBe(before.data.active_project);
  expect(undone.can_undo).toBe(false);
});

test('an unsupported saved progress schema is retained, explicitly recovered by workspace import, and honestly restored by undo', async ({ page }, testInfo) => {
  await start(page);
  const original = await downloadJson(page, page.getByRole('button', { name: 'Export all local work', exact: true }), testInfo, 'valid-workspace-before-corruption.json');
  const before = await envelope(page);
  const corrupt = structuredClone(before.data);
  corrupt.progress.schema = 999;
  const corrupted = await page.evaluate(async ({ revision, data }) => {
    const api = await import('/nibli-learn/platform/platform.js');
    const result = JSON.parse(await api.saveWorkspace(revision, JSON.stringify(data), false));
    if (!result.ok) throw new Error(result.error.message);
    return result.value;
  }, { revision: before.revision, data: corrupt });
  await page.reload();
  await ready(page);
  await expect(page.getByRole('alert').filter({ hasText: /retained|incompatible progress/i }).first()).toBeVisible();
  await expect(page.getByRole('button', { name: 'Saved on this device', exact: true })).toHaveCount(0);
  const retained = await envelope(page, false);
  expect(retained.revision).toBe(corrupted.revision);
  expect(retained.data).toEqual(corrupt);
  const rawBefore = await downloadJson(page, page.getByRole('button', { name: 'Export retained raw data', exact: true }), testInfo, 'unsupported-progress-retained-raw.json');
  expect(rawBefore.workspace.revision).toBe(corrupted.revision);
  expect(rawBefore.workspace.data).toEqual(corrupt);

  await preview(page, 'workspace', original);
  await expect(page.getByRole('heading', { name: 'Validated preview', exact: true })).toBeVisible();
  expect((await envelope(page, false)).data).toEqual(corrupt);
  await page.getByRole('button', { name: 'Apply this validated import', exact: true }).click();
  await ready(page);
  await expect.poll(async () => (await envelope(page, false)).data.progress.schema).toBe(2);
  const recovered = await envelope(page);
  expect(BigInt(recovered.revision)).toBeGreaterThan(BigInt(corrupted.revision));
  expect(recovered.can_undo).toBe(true);
  expect(recovered.data.progress).toEqual(original.progress);
  await dataPage(page);
  const rawRecovered = await downloadJson(page, page.getByRole('button', { name: 'Export retained raw data', exact: true }), testInfo, 'recovery-retained-invalid-checkpoint.json');
  expect(rawRecovered.backup.data).toEqual(corrupt);
  expect(rawRecovered.backup.revision).toBe(corrupted.revision);
  await page.getByRole('button', { name: 'Undo last replacement', exact: true }).click();
  await expect.poll(async () => (await envelope(page, false)).data.progress.schema).toBe(999);
  await expect(page.getByRole('alert').filter({ hasText: /retained|incompatible progress/i }).first()).toBeVisible();
  await expect(page.getByRole('button', { name: 'Saved on this device', exact: true })).toHaveCount(0);
  const restoredInvalid = await envelope(page, false);
  expect(restoredInvalid.data).toEqual(corrupt);
  expect(restoredInvalid.can_undo).toBe(false);
  await page.getByTestId('theme-toggle').click();
  const rawAfterUndo = await downloadJson(page, page.getByRole('button', { name: 'Export retained raw data', exact: true }), testInfo, 'undo-invalid-checkpoint-stays-retained.json');
  expect(rawAfterUndo.workspace.revision).toBe(restoredInvalid.revision);
  expect(rawAfterUndo.workspace.data).toEqual(corrupt);
  await page.reload();
  await ready(page);
  await expect(page.getByRole('alert').filter({ hasText: /retained|incompatible progress/i }).first()).toBeVisible();
  expect((await envelope(page, false)).data).toEqual(corrupt);
  expect((await envelope(page, false)).revision).toBe(restoredInvalid.revision);
});

test('explicit import from a stale tab cannot bypass a newer writer by using the recovery path', async ({ page, context }, testInfo) => {
  await start(page);
  const before = await envelope(page);
  const source = await downloadJson(page, page.getByRole('button', { name: 'Export all local work', exact: true }), testInfo, 'stale-import-original-workspace.json');
  await preview(page, 'workspace', source);
  await expect(page.getByRole('heading', { name: 'Validated preview', exact: true })).toBeVisible();
  const writer = await context.newPage();
  await writer.goto('/nibli-learn/#/data');
  await ready(writer);
  await envelope(writer);
  const id = before.data.active_project;
  await rename(writer, before.data.projects[id].name, 'Newer writer must survive import');
  await expect.poll(async () => (await envelope(writer, false)).data.projects[id].name).toBe('Newer writer must survive import');
  const committed = await envelope(writer);
  await expect(page.getByText('Another tab has newer saved work. Current unsaved changes have not been overwritten.', { exact: true })).toBeVisible();
  const apply = page.getByRole('button', { name: 'Apply this validated import', exact: true });
  await expect(apply).toBeVisible();
  if (await apply.isEnabled()) await apply.click();
  else await expect(apply).toBeDisabled();
  await expect(page.getByRole('alert').filter({ hasText: /another tab|newer|reload/i }).first()).toBeVisible();
  const unchanged = await envelope(page, false);
  expect(unchanged.revision).toBe(committed.revision);
  expect(unchanged.data).toEqual(committed.data);
  const candidate = await downloadJson(page, page.getByRole('button', { name: 'Export this tab’s copy', exact: true }), testInfo, 'stale-import-retained-tab-copy.json');
  expect(candidate.projects[id].name).toBe(before.data.projects[id].name);
  await page.getByRole('button', { name: 'Reload saved workspace', exact: true }).click();
  await ready(page);
  expect((await envelope(page)).data.projects[id].name).toBe('Newer writer must survive import');
  await writer.close();
});

import { test, expect } from '@playwright/test';
import { mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('..', import.meta.url));
const app = path.join(root, 'test-results/platform-site/nibli-learn');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const jsonBytes = value => Buffer.from(JSON.stringify(value));
async function put(relative, bytes) { const name = path.join(app, relative); await mkdir(path.dirname(name), { recursive: true }); await writeFile(name, bytes); }
async function fixture(label, { changedContent = false, version = 2, unsafe = false, localEngine = false, development = false } = {}) {
  const id = `runtime-${label}`;
  const runtimeBase = `/nibli-learn/runtime/${id}/`;
  const packBase = `/nibli-learn/packs/nibli-core/${version}/`;
  const runtimePayload = {
    'index.html': Buffer.from(`<!doctype html><title>Runtime ${label}</title><h1 id="runtime">Runtime ${label}</h1><script type="module" src="${runtimeBase}app.js"></script>`),
    'app.js': Buffer.from(`window.runtimeVersion=${JSON.stringify(label)};window.platform=await import('./platform/platform.js');`),
    'platform/platform.js': await readFile(path.join(root, 'crates/client/js/platform.js')),
    'worker/bootstrap.js': Buffer.from(`self.onmessage=()=>self.postMessage(${JSON.stringify(label)});`),
    'worker/pkg/nibli_learn_worker.js': Buffer.from('export default async function init() {}'),
    'worker/pkg/nibli_learn_worker_bg.wasm': Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]),
  };
  const course = JSON.parse(await readFile(path.join(root, 'content/course.json'), 'utf8'));
  const references = JSON.parse(await readFile(path.join(root, 'content/references.json'), 'utf8'));
  course.version = references.version = version;
  if (changedContent) course.lessons[0].summary += ' Changed bytes without a content-version bump.';
  const contentPayload = {
    'manifest.json': jsonBytes({ schema: 2, kind: 'nibli-learn-content-manifest', content_version: version, minimum_app_schema: 2 }),
    'course.json': jsonBytes(course), 'references.json': jsonBytes(references),
  };
  async function files(payload, base) {
    const result = [];
    for (const [name, bytes] of Object.entries(payload).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
      await put(base.replace('/nibli-learn/', '') + name, bytes);
      result.push({ path: name, sha256: hash(bytes), bytes: bytes.length, media_type: name.endsWith('.json') ? 'application/json' : name.endsWith('.js') ? 'text/javascript' : name.endsWith('.wasm') ? 'application/wasm' : 'text/html' });
    }
    return result;
  }
  const engineIdentity = localEngine ? `local-${'a'.repeat(40)}-source-${'c'.repeat(16)}` : 'a'.repeat(40);
  const runtime = { schema: 1, kind: 'nibli-runtime', id, app_schema: 2, app_revision: 'b'.repeat(40), engine_revision: engineIdentity, development, base_path: runtimeBase, files: await files(runtimePayload, runtimeBase) };
  const packFiles = await files(contentPayload, packBase);
  const digest = hash(jsonBytes(packFiles.map(({ path, sha256, bytes }) => ({ path, sha256, bytes }))));
  const pack = { schema: 1, kind: 'nibli-content-pack', id: 'nibli-core', version, content_version: version, app_schema: 2, engine_revision: engineIdentity, development, base_path: packBase, content_manifest: 'manifest.json', content_digest: digest, files: packFiles };
  if (unsafe) runtime.files[0].path = '../escape.js';
  const runtimeBytes = jsonBytes(runtime); const packBytes = jsonBytes(pack);
  await put(`offline/runtime-${label}.json`, runtimeBytes); await put(`offline/pack-${label}.json`, packBytes);
  const catalog = { schema: 1, kind: 'nibli-offline-catalog', entries: [{ id, title: `Fixture ${label}`, app_schema: 2, development,
    runtime: { url: `/nibli-learn/offline/runtime-${label}.json`, sha256: hash(runtimeBytes) },
    content: { url: `/nibli-learn/offline/pack-${label}.json`, sha256: hash(packBytes) } }] };
  await put('offline/catalog.json', jsonBytes(catalog));
  return { id, runtimeBase, runtimePayload };
}
async function open(page) {
  await page.goto('/nibli-learn/');
  await page.evaluate(async () => { window.platform = await import('/nibli-learn/platform/platform.js'); });
}
async function call(page, method, ...args) {
  return page.evaluate(async ({ method, args }) => JSON.parse(await window.platform[method](...args)), { method, args });
}
async function ok(page, method, ...args) {
  const result = await call(page, method, ...args);
  if (!result.ok) throw new Error(`${result.error.kind}: ${result.error.message}`);
  return result.value;
}
async function register(page) {
  await ok(page, 'registerServiceWorker');
  await page.evaluate(() => navigator.serviceWorker.ready);
}
async function install(page, item) { return ok(page, 'installOffline', item.id, `job-${item.id}`, null); }
async function activeFixture(page, label = 'a') {
  const item = await fixture(label); await open(page); await register(page);
  await install(page, item); await ok(page, 'activateOffline', item.id, null); return item;
}
async function workerVersion(page, base) {
  return page.evaluate(base => new Promise((resolve, reject) => {
    const worker = new Worker(`${base}worker/bootstrap.js`, { type: 'module' });
    worker.onmessage = event => { worker.terminate(); resolve(event.data); };
    worker.onerror = event => { worker.terminate(); reject(new Error(event.message)); };
    worker.postMessage('identify');
  }), base);
}

test('CAS saves, explicit import checkpoints and undo preserve the exact legacy bytes', async ({ page }) => {
  await open(page);
  await page.evaluate(() => localStorage.setItem('nibli-learn.local.v1', '{original unreadable learner work'));
  expect(await ok(page, 'loadWorkspace')).toBeNull();
  const one = { schema: 2, progress: { label: 'original' } };
  const saved = await ok(page, 'saveWorkspace', null, JSON.stringify(one), false);
  expect(saved.revision).toBe('1');
  const imported = await ok(page, 'saveWorkspace', '1', JSON.stringify({ schema: 2, progress: { label: 'imported' } }), true);
  expect(imported.can_undo).toBe(true);
  await ok(page, 'saveWorkspace', '2', JSON.stringify({ schema: 2, progress: { label: 'ordinary edit' } }), false);
  const undone = await ok(page, 'undoWorkspace', '3');
  expect(undone.data).toEqual(one); expect(undone.revision).toBe('4'); expect(undone.can_undo).toBe(false);
  const rejected = await call(page, 'saveWorkspace', '4', JSON.stringify({ schema: 999 }), false);
  expect(rejected.error.kind).toBe('invalid');
  expect((await ok(page, 'loadWorkspace')).data).toEqual(one);
  const raw = await ok(page, 'rawExistingData');
  expect(raw.legacy.raw).toBe('{original unreadable learner work'); expect(raw.legacy_backup.raw).toBe(raw.legacy.raw);
  expect(raw.before_undo.data.progress.label).toBe('ordinary edit');
});

test('two tabs reject stale writes, notify the other tab and retain its unsaved candidate', async ({ page, context }) => {
  await open(page); const other = await context.newPage(); await open(other);
  await other.evaluate(() => { window.changes = []; window.platform.subscribeWorkspace(text => window.changes.push(JSON.parse(text))); window.unsaved = { schema: 2, label: 'second tab candidate' }; });
  await ok(page, 'saveWorkspace', null, JSON.stringify({ schema: 2, label: 'first tab' }), false);
  await expect.poll(() => other.evaluate(() => window.changes.filter(value => !value.own && value.kind === 'workspace_changed').length)).toBe(1);
  const conflict = await other.evaluate(async () => JSON.parse(await window.platform.saveWorkspace(null, JSON.stringify(window.unsaved), false)));
  expect(conflict.error.kind).toBe('conflict'); expect(conflict.error.current_revision).toBe('1');
  expect(await other.evaluate(() => window.unsaved.label)).toBe('second tab candidate');
  expect((await ok(other, 'loadWorkspace')).data.label).toBe('first tab');
});

test('a controlled quota failure aborts both the save and backup replacement', async ({ page }) => {
  await open(page); await ok(page, 'saveWorkspace', null, JSON.stringify({ schema: 2, label: 'retained' }), false);
  await page.evaluate(() => {
    const original = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function(value, key) { if (key === 'workspace') throw new DOMException('Controlled quota failure', 'QuotaExceededError'); return original.call(this, value, key); };
  });
  const result = await call(page, 'saveWorkspace', '1', JSON.stringify({ schema: 2, label: 'not committed' }), true);
  expect(result.error.kind).toBe('quota');
  const retained = await ok(page, 'loadWorkspace'); expect(retained.data.label).toBe('retained'); expect(retained.can_undo).toBe(false);
  expect((await ok(page, 'rawExistingData')).workspace.revision).toBe('1');
});

test('only verified activation enables a new offline page and its versioned worker', async ({ page, context }) => {
  const item = await fixture('a'); await open(page); await register(page);
  expect((await ok(page, 'offlineStatus')).installed).toBe(false);
  const downloaded = await install(page, item); expect(downloaded.state).toBe('verified');
  expect((await ok(page, 'offlineStatus')).installed).toBe(false);
  const content = await ok(page, 'installedContent', item.id);
  expect(content.course.lessons).toHaveLength(10); expect(content.references.entries.length).toBeGreaterThan(40);
  await ok(page, 'activateOffline', item.id, null);
  expect((await ok(page, 'offlineStatus')).installed).toBe(true);
  await context.setOffline(true); const fresh = await context.newPage(); await fresh.goto('/nibli-learn/');
  await expect(fresh.locator('#runtime')).toHaveText('Runtime a');
  expect(await workerVersion(fresh, item.runtimeBase)).toBe('a');
});

test('missing, corrupt and cancelled updates retain the previous active installation', async ({ page }) => {
  await activeFixture(page); const newer = await fixture('b');
  const payloadPath = `runtime/${newer.id}/app.js`;
  await rm(path.join(app, payloadPath));
  expect((await call(page, 'installOffline', newer.id, 'missing', null)).error.kind).toBe('network');
  await put(payloadPath, Buffer.from('corrupt bytes'));
  expect((await call(page, 'installOffline', newer.id, 'corrupt', null)).error.kind).toBe('integrity');
  await put(payloadPath, newer.runtimePayload['app.js']);
  const cancelled = await page.evaluate(async id => JSON.parse(await window.platform.installOffline(id, 'cancel-update', text => {
    if (JSON.parse(text).completed === 1) window.platform.cancelInstallation('cancel-update');
  })), newer.id);
  expect(cancelled.error.kind).toBe('cancelled');
  const status = await ok(page, 'offlineStatus'); expect(status.active.installation_id).toBe('runtime-a'); expect(status.installed).toBe(true);
  expect(status.installations).toHaveLength(1);
});

test('updates and rollback preserve old runtime URLs and reject stale activation', async ({ page, context }) => {
  const old = await activeFixture(page); const newer = await fixture('b'); await install(page, newer);
  const updated = await ok(page, 'activateOffline', newer.id, '1'); expect(updated.revision).toBe('2');
  expect((await call(page, 'activateOffline', old.id, '1')).error.kind).toBe('conflict');
  await context.setOffline(true);
  expect(await workerVersion(page, old.runtimeBase)).toBe('a');
  const fresh = await context.newPage(); await fresh.goto('/nibli-learn/'); await expect(fresh.locator('#runtime')).toHaveText('Runtime b');
  expect(await workerVersion(fresh, newer.runtimeBase)).toBe('b');
  await ok(page, 'undoOffline', '2');
  const restored = await context.newPage(); await restored.goto('/nibli-learn/'); await expect(restored.locator('#runtime')).toHaveText('Runtime a');
  expect(await workerVersion(fresh, newer.runtimeBase)).toBe('b');
});

test('changed bytes at the same content version and future content are refused', async ({ page }) => {
  await activeFixture(page);
  const changed = await fixture('changed', { changedContent: true });
  expect((await call(page, 'installOffline', changed.id, 'changed-content', null)).error.kind).toBe('version_conflict');
  const future = await fixture('future', { version: 3 });
  expect((await call(page, 'installOffline', future.id, 'future-content', null)).error.kind).toBe('incompatible');
  expect((await ok(page, 'offlineStatus')).active.installation_id).toBe('runtime-a');
});

test('unsafe paths and corrupted cached bytes cannot be activated or called installed', async ({ page }) => {
  await activeFixture(page);
  const unsafe = await fixture('unsafe', { unsafe: true });
  expect((await call(page, 'installOffline', unsafe.id, 'unsafe-path', null)).error.kind).toBe('invalid');
  await page.evaluate(async () => {
    const status = JSON.parse(await window.platform.offlineStatus()).value;
    const installed = status.installations[0]; const cache = await caches.open(installed.cache_name);
    await cache.put(installed.files[0].url, new Response('changed cache bytes'));
  });
  expect((await ok(page, 'offlineStatus')).installed).toBe(false);
  expect((await call(page, 'activateOffline', 'runtime-a', '1')).error.kind).toBe('integrity');
});

test('blocked storage preserves raw legacy export and reports an explicit recovery error', async ({ page }) => {
  await open(page);
  await page.evaluate(() => {
    localStorage.setItem('nibli-learn.local.v1', 'exact legacy bytes');
    IDBFactory.prototype.open = function() { throw new DOMException('Controlled browser storage denial', 'SecurityError'); };
  });
  expect((await call(page, 'loadWorkspace')).error.kind).toBe('blocked');
  const exported = await ok(page, 'rawExistingData');
  expect(exported.legacy.raw).toBe('exact legacy bytes');
  expect(exported.storage_error.kind).toBe('blocked');
});

test('cache quota and activation transaction failures retain the previous complete version', async ({ page }) => {
  await activeFixture(page); const newer = await fixture('b');
  await page.evaluate(() => {
    window.originalCachePut = Cache.prototype.put;
    Cache.prototype.put = function() { throw new DOMException('Controlled cache quota failure', 'QuotaExceededError'); };
  });
  expect((await call(page, 'installOffline', newer.id, 'cache-quota', null)).error.kind).toBe('quota');
  expect((await ok(page, 'offlineStatus')).installations).toHaveLength(1);
  expect(await page.evaluate(() => caches.keys())).toHaveLength(1);
  await page.evaluate(() => { Cache.prototype.put = window.originalCachePut; });
  await install(page, newer);
  await page.evaluate(() => {
    const original = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function(value, key) { if (key === 'active') throw new DOMException('Controlled activation quota failure', 'QuotaExceededError'); return original.call(this, value, key); };
  });
  expect((await call(page, 'activateOffline', newer.id, '1')).error.kind).toBe('quota');
  const status = await ok(page, 'offlineStatus');
  expect(status.active.installation_id).toBe('runtime-a'); expect(status.active.revision).toBe('1'); expect(status.installed).toBe(true);
});

test('closing a downloading tab cannot publish partial bytes or replace the active runtime', async ({ page, context }) => {
  const old = await activeFixture(page); const newer = await fixture('b');
  const interrupted = await context.newPage(); await open(interrupted);
  const delay = path.join(app, '../delay-path.txt');
  const started = path.join(app, '../delay-started');
  await writeFile(delay, `${newer.runtimeBase}index.html`);
  await interrupted.evaluate(id => { window.pendingInstall = window.platform.installOffline(id, 'interrupted-install', null); }, newer.id);
  await expect.poll(async () => readFile(started).then(() => true, () => false)).toBe(true);
  await interrupted.close(); await rm(delay);
  const status = await ok(page, 'offlineStatus');
  expect(status.active.installation_id).toBe(old.id); expect(status.installed).toBe(true); expect(status.installations).toHaveLength(1);
  await context.setOffline(true); const fresh = await context.newPage(); await fresh.goto('/nibli-learn/');
  await expect(fresh.locator('#runtime')).toHaveText('Runtime a');
  expect(await workerVersion(fresh, old.runtimeBase)).toBe('a');
});

test('a missing published asset fails closed instead of receiving unchecked replacement bytes', async ({ page }) => {
  const item = await activeFixture(page);
  await page.evaluate(async () => {
    const installed = JSON.parse(await window.platform.offlineStatus()).value.installations[0];
    await (await caches.open(installed.cache_name)).delete(`${location.origin}${installed.runtime_base}app.js`);
  });
  await put(`runtime/${item.id}/app.js`, Buffer.from('unchecked network replacement'));
  const response = await page.evaluate(async base => { const result = await fetch(`${base}app.js`); return { status: result.status, text: await result.text() }; }, item.runtimeBase);
  expect(response.status).toBe(503); expect(response.text).not.toContain('unchecked network replacement');
  expect((await ok(page, 'offlineStatus')).installed).toBe(false);
});

test('local engine snapshots require explicit development metadata and keep that identity', async ({ page }) => {
  await open(page); await register(page);
  const unmarked = await fixture('unmarked', { localEngine: true });
  expect((await call(page, 'installOffline', unmarked.id, 'unmarked-local', null)).error.kind).toBe('incompatible');
  const marked = await fixture('development', { localEngine: true, development: true });
  const downloaded = await install(page, marked);
  expect(downloaded.development).toBe(true);
  expect(downloaded.engine_revision).toBe(`local-${'a'.repeat(40)}-source-${'c'.repeat(16)}`);
  expect((await ok(page, 'offlineStatus')).installed).toBe(false);
});

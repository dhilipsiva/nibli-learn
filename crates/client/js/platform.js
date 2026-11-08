// Browser persistence and offline installation. This module never evaluates lesson code.
const DB_NAME = 'nibli-learn.platform.v2';
const DB_VERSION = 1;
const BASE = '/nibli-learn/';
const LEGACY_KEY = 'nibli-learn.local.v1';
const MAX_WORKSPACE_BYTES = 16 * 1024 * 1024;
const MAX_FILE_BYTES = 128 * 1024 * 1024;
const MAX_INSTALL_BYTES = 512 * 1024 * 1024;
const MAX_FILES = 2048;
const encoder = new TextEncoder();
const actor = globalThis.crypto?.randomUUID?.() ?? `tab-${Date.now()}-${Math.random()}`;
let connection;
let opening;
let channel;
const subscribers = new Map();
const jobs = new Map();

function fault(kind, message, current_revision = undefined) {
  return Object.assign(new Error(message), { kind, current_revision });
}
function mapped(error) {
  if (error?.kind) return { kind: error.kind, message: error.message, ...(error.current_revision !== undefined ? { current_revision: error.current_revision } : {}) };
  const name = error?.name;
  const kind = name === 'QuotaExceededError' ? 'quota' : name === 'AbortError' ? 'cancelled'
    : ['SecurityError', 'NotAllowedError'].includes(name) ? 'blocked' : 'unavailable';
  return { kind, message: error?.message || 'Browser storage is unavailable.' };
}
async function wire(operation) {
  try { return JSON.stringify({ ok: true, value: await operation() }); }
  catch (error) { return JSON.stringify({ ok: false, error: mapped(error) }); }
}
function revision(value) {
  if (value !== null && (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value) || BigInt(value) > 18446744073709551615n)) {
    throw fault('invalid', 'Storage revision must be a canonical unsigned decimal string.');
  }
  return value;
}
function nextRevision(current) {
  const next = BigInt(current ?? '0') + 1n;
  if (next > 18446744073709551615n) throw fault('invalid', 'Storage revision exhausted.');
  return next.toString();
}
function validateData(data) {
  if (!data || typeof data !== 'object' || Array.isArray(data) || data.schema !== 2) {
    throw fault('invalid', 'Workspace data must use schema 2. Existing work was retained.');
  }
  if (encoder.encode(JSON.stringify(data)).byteLength > MAX_WORKSPACE_BYTES) {
    throw fault('too_large', 'Workspace exceeds 16 MiB. Existing work was retained; export a smaller project.');
  }
}
function validateEnvelope(value) {
  if (value === undefined || value === null) return null;
  if (value.schema !== 2 || revision(value.revision) === null) throw fault('invalid', 'Saved workspace envelope is incompatible. Existing bytes were retained.');
  validateData(value.data);
  return value;
}
function openDb() {
  if (connection) return Promise.resolve(connection);
  if (opening) return opening;
  opening = new Promise((resolve, reject) => {
    if (!globalThis.indexedDB) { reject(fault('unavailable', 'IndexedDB is unavailable. Export local work before leaving this page.')); return; }
    let settled = false;
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      for (const name of ['records', 'installs', 'meta', 'content_versions']) {
        if (!db.objectStoreNames.contains(name)) db.createObjectStore(name);
      }
    };
    request.onblocked = () => { settled = true; reject(fault('blocked', 'Another tab blocks the storage upgrade. Close that tab and retry; existing work was retained.')); };
    request.onerror = () => { settled = true; reject(request.error); };
    request.onsuccess = () => {
      if (settled) { request.result.close(); return; }
      connection = request.result;
      connection.onversionchange = () => { connection?.close(); connection = undefined; opening = undefined; notify({ kind: 'storage_version_changed' }); };
      resolve(connection);
    };
  }).finally(() => { opening = undefined; });
  return opening;
}
async function readRecord(store, key) {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, 'readonly');
    const request = tx.objectStore(store).get(key);
    let result;
    request.onsuccess = () => { result = request.result; };
    tx.oncomplete = () => resolve(result);
    tx.onabort = tx.onerror = () => reject(tx.error || request.error || fault('unavailable', 'Storage read failed.'));
  });
}
function legacyData() {
  try { return { key: LEGACY_KEY, raw: localStorage.getItem(LEGACY_KEY) }; }
  catch (error) { return { key: LEGACY_KEY, raw: null, error: mapped(error) }; }
}
function ensureChannel() {
  if (!channel && globalThis.BroadcastChannel) {
    channel = new BroadcastChannel('nibli-learn.platform.v2');
    channel.onmessage = event => dispatch({ ...event.data, own: event.data.actor === actor });
  }
}
function dispatch(event) {
  for (const callback of subscribers.values()) { try { callback(JSON.stringify(event)); } catch { /* One UI callback cannot invalidate a committed save. */ } }
}
function notify(event) {
  ensureChannel();
  const message = { ...event, actor };
  channel?.postMessage(message);
  dispatch({ ...message, own: true });
}
export function subscribeWorkspace(callback) {
  ensureChannel();
  const token = crypto.randomUUID();
  subscribers.set(token, callback);
  return token;
}
export function unsubscribeWorkspace(token) { subscribers.delete(token); }

export function loadWorkspace() {
  return wire(async () => {
    const db = await openDb();
    return new Promise((resolve, reject) => {
      const tx = db.transaction('records', 'readonly');
      const current = tx.objectStore('records').get('workspace');
      const backup = tx.objectStore('records').get('backup');
      tx.oncomplete = () => {
        try { const value = validateEnvelope(current.result); resolve(value ? { ...value, can_undo: !!backup.result } : null); }
        catch (error) { reject(error); }
      };
      tx.onabort = tx.onerror = () => reject(tx.error || fault('unavailable', 'Workspace read failed.'));
    });
  });
}
export function saveWorkspace(expected, json, backup) {
  return wire(async () => {
    revision(expected);
    if (typeof json !== 'string' || encoder.encode(json).byteLength > MAX_WORKSPACE_BYTES) throw fault('too_large', 'Workspace exceeds 16 MiB or is not serialized text. Existing work was retained.');
    let data;
    try { data = JSON.parse(json); } catch { throw fault('invalid', 'Workspace is not valid JSON.'); }
    validateData(data);
    const legacy = legacyData();
    const db = await openDb();
    const receipt = await new Promise((resolve, reject) => {
      const tx = db.transaction('records', 'readwrite');
      const records = tx.objectStore('records');
      let result;
      let failure;
      const fail = error => { failure = error; tx.abort(); };
      const request = records.get('workspace');
      request.onsuccess = () => {
        try {
          const current = validateEnvelope(request.result);
          if ((current?.revision ?? null) !== expected) throw fault('conflict', 'Another tab saved newer work. Your candidate remains unsaved; reload or export it before replacing anything.', current?.revision ?? null);
          result = { schema: 2, revision: nextRevision(current?.revision), data, updated_at: new Date().toISOString() };
          if (backup && current) records.put(current, 'backup');
          records.put(result, 'workspace');
          const existingLegacy = records.get('legacy_backup');
          existingLegacy.onsuccess = () => {
            try { if (existingLegacy.result === undefined && legacy.raw !== null) records.put(legacy, 'legacy_backup'); }
            catch (error) { fail(error); }
          };
          const existingBackup = records.get('backup');
          existingBackup.onsuccess = () => { result.can_undo = !!existingBackup.result; };
        } catch (error) { fail(error); }
      };
      tx.oncomplete = () => resolve(result);
      tx.onabort = tx.onerror = () => reject(failure || tx.error || fault('unavailable', 'Workspace save failed. Existing work was retained.'));
    });
    notify({ kind: 'workspace_changed', revision: receipt.revision });
    return receipt;
  });
}
export function undoWorkspace(expected) {
  return wire(async () => {
    revision(expected);
    const db = await openDb();
    const receipt = await new Promise((resolve, reject) => {
      const tx = db.transaction('records', 'readwrite');
      const records = tx.objectStore('records');
      let result;
      let failure;
      const currentRequest = records.get('workspace');
      currentRequest.onsuccess = () => {
        try {
          const current = validateEnvelope(currentRequest.result);
          if (!current || current.revision !== expected) throw fault('conflict', 'Newer work exists. Preview it before undoing an import.', current?.revision ?? null);
          const backupRequest = records.get('backup');
          backupRequest.onsuccess = () => {
            try {
              const backup = validateEnvelope(backupRequest.result);
              if (!backup) throw fault('unavailable', 'There is no retained import checkpoint to restore.');
              result = { schema: 2, revision: nextRevision(current.revision), data: backup.data, updated_at: new Date().toISOString(), can_undo: false };
              records.put(current, 'before_undo');
              records.put(result, 'workspace');
              records.delete('backup');
            } catch (error) { failure = error; tx.abort(); }
          };
        } catch (error) { failure = error; tx.abort(); }
      };
      tx.oncomplete = () => resolve(result);
      tx.onabort = tx.onerror = () => reject(failure || tx.error || fault('unavailable', 'Undo failed. Existing work was retained.'));
    });
    notify({ kind: 'workspace_changed', revision: receipt.revision });
    return receipt;
  });
}
export function rawExistingData() {
  return wire(async () => {
    const result = { schema: 1, kind: 'nibli-raw-local-recovery', legacy: legacyData() };
    try {
      const db = await openDb();
      await new Promise((resolve, reject) => {
        const tx = db.transaction('records', 'readonly');
        for (const key of ['workspace', 'backup', 'legacy_backup', 'before_undo']) {
          const request = tx.objectStore('records').get(key);
          request.onsuccess = () => { result[key] = request.result ?? null; };
        }
        tx.oncomplete = () => resolve();
        tx.onabort = tx.onerror = () => reject(tx.error || fault('unavailable', 'Raw recovery read failed.'));
      });
    } catch (error) { result.storage_error = mapped(error); }
    return result;
  });
}

// Offline methods are defined below; manifests are separate from their payload hashes.
const CACHE_PREFIX = 'nibli-learn-v2-';
const CATALOG_URL = `${BASE}offline/catalog.json`;
const SHA = /^[a-f0-9]{64}$/;
const ID = /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,191}$/;
const PACK_ID = /^[a-z][a-z0-9.-]{0,127}$/;
function sameOriginUrl(path) {
  const url = new URL(path, location.origin);
  if (url.origin !== location.origin || !url.pathname.startsWith(BASE) || url.username || url.password || url.search || url.hash) {
    throw fault('invalid', 'Offline assets must use the application’s first-party mount.');
  }
  return url.href;
}
function safePath(path) {
  if (typeof path !== 'string' || path.length > 512 || !/^[a-zA-Z0-9_-][a-zA-Z0-9_./-]*$/.test(path)
      || path.split('/').some(part => !part || part === '.' || part === '..')) {
    throw fault('invalid', 'An offline manifest contains an unsafe or duplicate path.');
  }
  return path;
}
async function sha256(bytes) {
  return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), byte => byte.toString(16).padStart(2, '0')).join('');
}
function parseObject(bytes, label) {
  try {
    const value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error();
    return value;
  } catch { throw fault('invalid', `${label} is not a valid JSON object.`); }
}
async function fetched(path, signal, maxBytes = MAX_FILE_BYTES) {
  const response = await fetch(sameOriginUrl(path), { cache: 'no-store', credentials: 'omit', redirect: 'error', signal });
  if (!response.ok) throw fault('network', `Offline asset is unavailable (HTTP ${response.status}).`);
  const declared = Number(response.headers.get('content-length'));
  if (declared > maxBytes) throw fault('too_large', 'An offline file exceeds the configured size limit.');
  const reader = response.body?.getReader();
  let bytes;
  if (reader) {
    const chunks = []; let size = 0;
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.byteLength;
        if (size > maxBytes) throw fault('too_large', 'An offline file exceeds the configured size limit.');
        chunks.push(value);
      }
    } catch (error) { await reader.cancel().catch(() => {}); throw error; }
    bytes = new Uint8Array(size); let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  } else {
    bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.length > maxBytes) throw fault('too_large', 'An offline file exceeds the configured size limit.');
  }
  return { bytes, headers: response.headers };
}
async function pinnedJson(reference, signal) {
  if (!reference || !SHA.test(reference.sha256)) throw fault('invalid', 'Catalog manifest hash is missing or malformed.');
  const result = await fetched(reference.url, signal, 4 * 1024 * 1024);
  if (await sha256(result.bytes) !== reference.sha256) throw fault('integrity', 'A manifest does not match the catalog’s pinned hash. The active installation was retained.');
  return { ...result, value: parseObject(result.bytes, 'Offline manifest') };
}
function validateFiles(manifest, isContent) {
  if (!Array.isArray(manifest.files) || !manifest.files.length || manifest.files.length > MAX_FILES) throw fault('invalid', 'Offline file inventory is invalid.');
  const seen = new Set(); let total = 0;
  for (const file of manifest.files) {
    safePath(file.path);
    if (seen.has(file.path) || !SHA.test(file.sha256) || !Number.isSafeInteger(file.bytes) || file.bytes < 0 || file.bytes > MAX_FILE_BYTES) throw fault('invalid', 'Offline file metadata is invalid.');
    seen.add(file.path); total += file.bytes;
    if (isContent && (!['application/json', 'text/markdown', 'image/png', 'image/jpeg', 'image/webp', 'image/avif'].includes(file.media_type)
      || !/\.(json|md|png|jpe?g|webp|avif)$/.test(file.path) || file.path === 'pack.json')) {
      throw fault('invalid', 'Lesson packs may contain declarative content and images, not executable code.');
    }
  }
  if (total > MAX_INSTALL_BYTES) throw fault('too_large', 'Offline installation exceeds the configured size limit.');
  return total;
}
function validateCatalog(catalog) {
  if (catalog.schema !== 1 || catalog.kind !== 'nibli-offline-catalog' || !Array.isArray(catalog.entries) || catalog.entries.length > 64) throw fault('invalid', 'Offline catalog schema is incompatible.');
  const ids = new Set();
  for (const entry of catalog.entries) {
    if (!ID.test(entry.id) || ids.has(entry.id) || ![2, 3, 4].includes(entry.app_schema) || !entry.runtime || !entry.content) throw fault('invalid', 'Offline catalog entry is invalid.');
    ids.add(entry.id);
    for (const ref of [entry.runtime, entry.content]) { sameOriginUrl(ref.url); if (!SHA.test(ref.sha256)) throw fault('invalid', 'Offline catalog lacks a valid manifest hash.'); }
  }
  return catalog;
}
async function catalogValue(allowRetained = true, signal = undefined) {
  try { return { catalog: validateCatalog(parseObject((await fetched(CATALOG_URL, signal, 1024 * 1024)).bytes, 'Offline catalog')), source: 'network' }; }
  catch (error) {
    if (!allowRetained || !['network', 'unavailable'].includes(mapped(error).kind)) throw error;
    const retained = await readRecord('meta', 'catalog');
    if (!retained) throw error;
    return { catalog: validateCatalog(retained), source: 'retained', notice: 'Showing the last verified catalog; current updates could not be checked.' };
  }
}
export function offlineCatalog() { return wire(() => catalogValue()); }
export function registerServiceWorker() {
  return wire(async () => {
    if (!globalThis.isSecureContext || !navigator.serviceWorker || !globalThis.caches) throw fault('unsupported', 'Verified offline installation needs service workers in a secure browser context.');
    let registration;
    try { registration = await navigator.serviceWorker.register(`${BASE}service-worker.js`, { scope: BASE, updateViaCache: 'none' }); }
    catch (error) { registration = await navigator.serviceWorker.getRegistration(BASE); if (!registration?.active) throw error; }
    return { scope: registration.scope, active: !!registration.active, update_waiting: !!registration.waiting };
  });
}
function checkCancelled(signal) { if (signal.aborted) throw fault('cancelled', 'Installation cancelled. The previous active version was retained.'); }
function progress(callback, value) { if (callback) { try { callback(JSON.stringify(value)); } catch { /* UI progress is not an installation transaction. */ } } }
async function installationComplete(installation, checkHashes = false) {
  if (!installation || !(await caches.has(installation.cache_name))) return false;
  const cache = await caches.open(installation.cache_name);
  for (const file of installation.files) {
    const response = await cache.match(file.url);
    if (!response) return false;
    if (checkHashes) {
      const bytes = new Uint8Array(await response.arrayBuffer());
      if (bytes.byteLength !== file.bytes || await sha256(bytes) !== file.sha256) return false;
    }
  }
  return true;
}
export function installOffline(entryId, jobId, callback) {
  return wire(async () => {
    if (!ID.test(jobId) || jobs.has(jobId)) throw fault('invalid', 'Installation job identity is invalid or already active.');
    if (!navigator.serviceWorker || !globalThis.caches) throw fault('unsupported', 'Offline storage is unavailable in this browser.');
    const controller = new AbortController(); jobs.set(jobId, controller);
    const cacheName = `${CACHE_PREFIX}staged-${crypto.randomUUID()}`;
    let committed = false;
    try {
      const { catalog } = await catalogValue(false, controller.signal);
      const entry = catalog.entries.find(row => row.id === entryId);
      if (!entry) throw fault('invalid', 'The requested installation is absent from the first-party catalog.');
      const runtimeResult = await pinnedJson(entry.runtime, controller.signal);
      const packResult = await pinnedJson(entry.content, controller.signal);
      const runtime = runtimeResult.value; const pack = packResult.value;
      const engineIdentityValid = /^[a-f0-9]{40}$/.test(runtime.engine_revision)
        || (runtime.development === true && pack.development === true && entry.development === true
          && /^local-[a-f0-9]{40}-source-[a-f0-9]{16}$/.test(runtime.engine_revision));
      if (runtime.schema !== 1 || runtime.kind !== 'nibli-runtime' || !ID.test(runtime.id) || ![2, 3, 4].includes(runtime.app_schema) || runtime.app_schema !== entry.app_schema || !engineIdentityValid
        || pack.schema !== 1 || pack.kind !== 'nibli-content-pack' || !PACK_ID.test(pack.id) || pack.app_schema !== runtime.app_schema || pack.engine_revision !== runtime.engine_revision
        || !Number.isSafeInteger(pack.version) || pack.version < 1 || ![2, 3, 4].includes(pack.content_version) || pack.content_version > pack.app_schema || !SHA.test(pack.content_digest)) {
        throw fault('incompatible', 'Runtime, engine or content versions are incompatible. The active installation was retained.');
      }
      if (runtime.base_path !== `${BASE}runtime/${runtime.id}/` || pack.base_path !== `${BASE}packs/${pack.id}/${pack.version}/`) throw fault('invalid', 'Manifest asset namespace does not match its versioned identity.');
      const requiredRuntime = ['index.html', 'worker/bootstrap.js', 'worker/pkg/nibli_learn_worker.js', 'worker/pkg/nibli_learn_worker_bg.wasm'];
      const totalBytes = validateFiles(runtime, false) + validateFiles(pack, true);
      if (totalBytes > MAX_INSTALL_BYTES || requiredRuntime.some(path => !runtime.files.some(file => file.path === path))
        || !['manifest.json', 'course.json', 'references.json'].every(path => pack.files.some(file => file.path === path)) || (pack.app_schema >= 3 && !pack.files.some(file => file.path === 'labs.json')) || pack.content_manifest !== 'manifest.json') {
        throw fault('invalid', 'A complete runtime and lesson content inventory is required.');
      }
      const digestBytes = encoder.encode(JSON.stringify([...pack.files].sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0).map(({ path, sha256, bytes }) => ({ path, sha256, bytes }))));
      if (await sha256(digestBytes) !== pack.content_digest) throw fault('integrity', 'Content inventory digest is inconsistent.');
      const contentKey = `${pack.id}@${pack.content_version}`;
      const oldDigest = await readRecord('content_versions', contentKey);
      if (oldDigest && oldDigest !== pack.content_digest) throw fault('version_conflict', 'This content version was previously installed with different bytes. A new content version is required.');
      const runtimeKey = `runtime_hash:${runtime.id}`;
      const oldRuntimeHash = await readRecord('meta', runtimeKey);
      if (oldRuntimeHash && oldRuntimeHash !== entry.runtime.sha256) throw fault('version_conflict', 'This runtime identity was previously verified with different bytes.');
      const estimate = await navigator.storage?.estimate?.().catch(() => null);
      if (estimate?.quota && estimate.quota - (estimate.usage || 0) < totalBytes) throw fault('quota', 'There is not enough browser storage to stage this installation. The active version was retained.');
      const cache = await caches.open(cacheName);
      const files = []; const contentJson = {};
      const inventory = [...runtime.files.map(file => ({ ...file, base: runtime.base_path, content: false })), ...pack.files.map(file => ({ ...file, base: pack.base_path, content: true }))];
      let completedBytes = 0;
      progress(callback, { job_id: jobId, phase: 'downloading', completed: 0, total: inventory.length, bytes: 0, total_bytes: totalBytes });
      for (const file of inventory) {
        checkCancelled(controller.signal);
        const url = sameOriginUrl(file.base + file.path);
        const { bytes, headers } = await fetched(url, controller.signal, file.bytes);
        if (bytes.byteLength !== file.bytes || await sha256(bytes) !== file.sha256) throw fault('integrity', `Offline bytes do not match the manifest for ${file.path}. The active installation was retained.`);
        if (file.content && file.path.endsWith('.json')) contentJson[file.path] = parseObject(bytes, file.path);
        const cachedHeaders = new Headers({ 'Content-Type': file.media_type || headers.get('content-type') || 'application/octet-stream', 'X-Content-Type-Options': 'nosniff' });
        await cache.put(url, new Response(bytes, { status: 200, headers: cachedHeaders }));
        files.push({ url, bytes: file.bytes, sha256: file.sha256 }); completedBytes += file.bytes;
        progress(callback, { job_id: jobId, phase: 'downloading', completed: files.length, total: inventory.length, bytes: completedBytes, total_bytes: totalBytes });
      }
      if (contentJson['manifest.json'].schema !== pack.app_schema || contentJson['manifest.json'].kind !== 'nibli-learn-content-manifest'
        || contentJson['manifest.json'].content_version !== pack.content_version || contentJson['manifest.json'].minimum_app_schema !== pack.app_schema
        || contentJson['course.json'].schema !== pack.app_schema || contentJson['course.json'].version !== pack.content_version
        || contentJson['references.json'].schema !== pack.app_schema || contentJson['references.json'].version !== pack.content_version) throw fault('incompatible', 'Content schemas or versions do not match the installation manifest.');
      checkCancelled(controller.signal);
      const installation = { schema: 1, id: entry.id, runtime_id: runtime.id, app_revision: runtime.app_revision, engine_revision: runtime.engine_revision, development: runtime.development === true,
        content_id: pack.id, content_version: pack.content_version, content_hash: pack.content_digest, content_base: pack.base_path,
        runtime_base: runtime.base_path, cache_name: cacheName, files, verified_at: new Date().toISOString(), runtime_manifest_hash: entry.runtime.sha256, content_manifest_hash: entry.content.sha256 };
      const db = await openDb();
      await new Promise((resolve, reject) => {
        const tx = db.transaction(['installs', 'content_versions', 'meta'], 'readwrite');
        let failure;
        const abort = () => { failure = fault('cancelled', 'Installation cancelled before publication.'); try { tx.abort(); } catch { /* transaction may already have committed */ } };
        controller.signal.addEventListener('abort', abort, { once: true });
        const old = tx.objectStore('content_versions').get(contentKey);
        old.onsuccess = () => {
          if (old.result && old.result !== pack.content_digest) { failure = fault('version_conflict', 'Another tab installed different bytes for this content version.'); tx.abort(); return; }
          const oldRuntime = tx.objectStore('meta').get(runtimeKey);
          oldRuntime.onsuccess = () => {
            try {
            if (oldRuntime.result && oldRuntime.result !== entry.runtime.sha256) { failure = fault('version_conflict', 'Another tab installed different bytes for this runtime identity.'); tx.abort(); return; }
            tx.objectStore('installs').put(installation, installation.id);
            tx.objectStore('content_versions').put(pack.content_digest, contentKey);
            tx.objectStore('meta').put(entry.runtime.sha256, runtimeKey);
            tx.objectStore('meta').put(catalog, 'catalog');
            } catch (error) { failure = error; tx.abort(); }
          };
        };
        tx.oncomplete = () => { controller.signal.removeEventListener('abort', abort); resolve(); };
        tx.onabort = tx.onerror = () => { controller.signal.removeEventListener('abort', abort); reject(failure || tx.error || fault('unavailable', 'Installation publication failed.')); };
      });
      committed = true;
      progress(callback, { job_id: jobId, phase: 'verified', completed: files.length, total: files.length, bytes: completedBytes, total_bytes: totalBytes });
      notify({ kind: 'installation_verified', installation_id: installation.id });
      return { ...installation, state: 'verified', active: false };
    } finally {
      jobs.delete(jobId);
      if (!committed) await caches.delete(cacheName).catch(() => {});
    }
  });
}
export function cancelInstallation(jobId) { const controller = jobs.get(jobId); if (!controller) return false; controller.abort(); return true; }
export function installedContent(installationId) {
  return wire(async () => {
    const installation = await readRecord('installs', installationId);
    if (!await installationComplete(installation, true)) throw fault('integrity', 'Verified installation files are missing or changed. Reinstall before activation.');
    const cache = await caches.open(installation.cache_name);
    const value = {};
    for (const name of ['manifest', 'course', 'references']) value[name] = await (await cache.match(sameOriginUrl(`${installation.content_base}${name}.json`))).json();
    const labs = await cache.match(sameOriginUrl(`${installation.content_base}labs.json`));
    if (labs) value.labs = await labs.json();
    return value;
  });
}
async function activate(installationId, expected) {
  revision(expected);
  const installation = await readRecord('installs', installationId);
  if (!await installationComplete(installation, true)) throw fault('integrity', 'Installation is incomplete or its cached bytes changed. The active version was retained.');
  const registration = await navigator.serviceWorker.getRegistration(BASE);
  if (!registration?.active) throw fault('unavailable', 'The offline service worker is not active yet. Retry after registration completes.');
  const db = await openDb();
  const receipt = await new Promise((resolve, reject) => {
    const tx = db.transaction('meta', 'readwrite'); const meta = tx.objectStore('meta'); let failure; let result;
    const current = meta.get('active');
    current.onsuccess = () => {
      try {
      if ((current.result?.revision ?? null) !== expected) { failure = fault('conflict', 'Another tab changed the active installation. Refresh its status before activating.', current.result?.revision ?? null); tx.abort(); return; }
      result = { schema: 1, revision: nextRevision(expected), installation_id: installation.id, runtime_id: installation.runtime_id, previous_id: current.result?.installation_id ?? null, activated_at: new Date().toISOString() };
      meta.put(result, 'active');
      } catch (error) { failure = error; tx.abort(); }
    };
    tx.oncomplete = () => resolve(result);
    tx.onabort = tx.onerror = () => reject(failure || tx.error || fault('unavailable', 'Activation failed. The prior version was retained.'));
  });
  registration.waiting?.postMessage({ kind: 'activate-worker' });
  notify({ kind: 'installation_activated', revision: receipt.revision, installation_id: receipt.installation_id, runtime_id: receipt.runtime_id });
  return receipt;
}
export function activateOffline(installationId, expected) { return wire(() => activate(installationId, expected)); }
export function undoOffline(expected) {
  return wire(async () => {
    const active = await readRecord('meta', 'active');
    if (!active?.previous_id) throw fault('unavailable', 'No earlier installation is retained for rollback.');
    return activate(active.previous_id, expected);
  });
}
export function offlineStatus() {
  return wire(async () => {
    const supported = !!(globalThis.isSecureContext && navigator.serviceWorker && globalThis.caches);
    if (!supported) return { supported: false, active: null, installations: [], installed: false };
    const active = (await readRecord('meta', 'active')) ?? null;
    const db = await openDb();
    const values = await new Promise((resolve, reject) => {
      const tx = db.transaction('installs', 'readonly'); const req = tx.objectStore('installs').getAll();
      req.onsuccess = () => resolve(req.result); req.onerror = () => reject(req.error);
    });
    const installations = [];
    for (const installation of values) installations.push({ ...installation, complete: await installationComplete(installation, true), active: active?.installation_id === installation.id });
    return { supported: true, active, installations, installed: !!installations.find(row => row.active && row.complete) };
  });
}

// Only verified, version-addressed installations are served from these caches.
const BASE = '/nibli-learn/';
const DB_NAME = 'nibli-learn.platform.v2';
function database() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 1);
    request.onupgradeneeded = () => {
      for (const name of ['records', 'installs', 'meta', 'content_versions']) {
        if (!request.result.objectStoreNames.contains(name)) request.result.createObjectStore(name);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error('Offline storage upgrade is blocked.'));
  });
}
async function installed() {
  const db = await database();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(['meta', 'installs'], 'readonly');
      const active = tx.objectStore('meta').get('active');
      const entries = tx.objectStore('installs').getAll();
      tx.oncomplete = () => resolve({ active: active.result, entries: entries.result });
      tx.onabort = tx.onerror = () => reject(tx.error);
    });
  } finally { db.close(); }
}
async function verifiedResponse(installation, url) {
  const file = installation.files.find(file => file.url === url);
  if (!file || !(await caches.has(installation.cache_name))) return null;
  const response = await (await caches.open(installation.cache_name)).match(url);
  if (!response) return null;
  const bytes = new Uint8Array(await response.clone().arrayBuffer());
  const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), byte => byte.toString(16).padStart(2, '0')).join('');
  if (bytes.length !== file.bytes || digest !== file.sha256) return new Response('Installed asset failed integrity verification. Restore or reinstall this version.', { status: 503, headers: { 'Content-Type': 'text/plain' } });
  return response;
}
self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));
self.addEventListener('message', event => {
  if (event.data?.kind === 'activate-worker') self.skipWaiting();
});
self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || url.origin !== self.location.origin || !url.pathname.startsWith(BASE)) return;
  const isEntry = event.request.mode === 'navigate' && [BASE, `${BASE}index.html`].includes(url.pathname);
  const isVersioned = url.pathname.startsWith(`${BASE}runtime/`) || url.pathname.startsWith(`${BASE}packs/`);
  if (!isEntry && !isVersioned) return;
  event.respondWith((async () => {
    try {
      const { active, entries } = await installed();
      if (isEntry && active) {
        const installation = entries.find(item => item.id === active.installation_id);
        if (installation) {
          const response = await verifiedResponse(installation, new URL(`${installation.runtime_base}index.html`, self.location.origin).href);
          if (response) return response;
          return new Response('The active offline installation is incomplete. Reconnect and restore a verified version.', { status: 503, headers: { 'Content-Type': 'text/plain' } });
        }
      }
      if (isVersioned) {
        for (const installation of entries) {
          if (!installation.files.some(file => file.url === url.href)) continue;
          const response = await verifiedResponse(installation, url.href);
          if (response) return response;
          return new Response('This verified asset is missing. Restore or reinstall this version.', { status: 503, headers: { 'Content-Type': 'text/plain' } });
        }
      }
    } catch { /* An uninstalled online page can still load from its actual origin. */ }
    try { return await fetch(event.request); }
    catch { return new Response('This version is not available offline. Install and verify it while connected.', { status: 503, headers: { 'Content-Type': 'text/plain' } }); }
  })());
});

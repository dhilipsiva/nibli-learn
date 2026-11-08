# Local workspace and offline platform contracts

`crates/client/src/platform.rs` exposes the browser platform to Rust. The JavaScript
module owns IndexedDB, CacheStorage and service-worker integration. The caller validates
the course, progress and project models; this layer validates envelope schema/size,
identities, integrity and compare-and-swap transactions. No lesson code is evaluated here.

## Workspace API

- `load_workspace()` returns an optional envelope with `schema: 2`, decimal-string
  `revision`, generic validated `data`, `updated_at` and `can_undo`.
- `save_workspace(expected_revision, data, backup)` requires an object with `schema: 2`
  and a maximum serialized size of 16 MiB. `None` means no prior workspace may exist.
  A mismatching revision rejects the entire transaction with `conflict` and the
  current revision. Callers serialize/coalesce their own saves and retain the unsaved
  candidate until acknowledgment.
- `backup: true` checkpoints the previous envelope for an explicitly confirmed import.
  Ordinary saves never overwrite this checkpoint. `undo_workspace(expected_revision)`
  atomically restores its data under a fresh revision, consumes that checkpoint and
  retains the displaced workspace as raw recovery data.
- `raw_existing_data()` exports the current envelope, retained checkpoint, pre-undo
  envelope and exact legacy bytes. It remains useful when model decoding fails.
- `subscribe_workspace_changes(callback)` returns a subscription released on drop.
  Events contain a kind, revision when applicable, an actor and `own` flag. Cross-tab
  events notify the UI; they never silently replace another tab's unsaved candidate.

The IndexedDB database is `nibli-learn.platform.v2`. Its internal database version is
independent of the workspace model schema. The legacy `nibli-learn.local.v1` localStorage
key is never rewritten or deleted. A successful first durable save retains its exact
bytes as an additional recovery record. A blocked upgrade, quota failure, invalid data
or stale write leaves the last committed workspace intact.

Errors distinguish `conflict`, `quota`, `blocked`, `invalid`, `too_large`, `unavailable`
and `unsupported`; the UI presents recovery/export without turning storage failure into
a reasoning outcome. The module sends no workspace data to a server or broadcast channel.

## Offline API and state transitions

`offline_catalog()` retrieves the first-party `/nibli-learn/offline/catalog.json`.
Each entry pins a runtime manifest and a separate content-pack manifest by SHA-256.
A retained verified catalog may be shown when disconnected, labeled as retained rather
than a successful update check. Installer downloads always use the current first-party
catalog and same-origin, mount-scoped URLs without credentials or redirects.

`register_service_worker()` registers `/nibli-learn/service-worker.js` with scope
`/nibli-learn/`. It reports actual registration state. A first installation must wait
until the worker is active before activation; an existing registration can continue
while offline. Service-worker updates do not automatically skip a waiting worker.

`install_offline(entry_id, job_id, progress_callback)` performs a staged download:

1. Verify catalog-pinned manifest bytes and compatible app/engine/content versions.
2. Validate safe unique paths, file types and bounded inventories (2048 files per
   manifest, 128 MiB per file, 512 MiB combined installation).
3. Download and verify every exact size/hash into an unpublished staging cache. Check
   course/reference schema and version against the pack. Content payloads cannot
   replace JavaScript, WASM or fonts.
4. Atomically publish the verified installation record and immutable content/runtime
   identities in IndexedDB. The active installation is unchanged.

The returned state is `verified`, with `active: false`. Progress reports
`job_id`, `phase`, `completed`, `total`, `bytes` and `total_bytes`.
`cancel_installation(job_id)` aborts a pending download/publication and deletes its
unpublished staging cache. Interrupted page termination can leave an inactive staging
cache, but it cannot change the active pointer or become an installed version.

Before applying an installation, call `installed_content(id)` and validate its `course`
and `references` using the Rust model's `Course::parse` and `ReferenceLibrary::parse`.
Schema 3 also requires a valid lab library whose course/reference links resolve.
Then the learner explicitly chooses activation. `activate_offline(id, expected_revision)`
rechecks all cached hashes and commits one compare-and-swap active pointer. It returns
the new revision, installation/runtime IDs, previous installation and activation time.
`undo_offline(expected_revision)` activates that retained previous installation through
the same checks. A failed or stale activation preserves the previous pointer.

`offline_status()` reports support, the active pointer, retained installations and
`installed`. Installed requires an active installation whose complete cache still
matches all expected hashes. Merely receiving bundled files or finishing a download
does not grant that state. Missing/corrupt caches require reinstall or a verified fallback.

The content digest hashes the ordered payload inventory (`path`, `sha256`, `bytes`),
excluding `pack.json` itself. The catalog supplies that manifest's expected hash.
An installed content ID/version can never later identify different payload bytes;
runtime identity likewise cannot be rebound to another manifest. Paired content/app
schemas 2, 3 and 4 are supported; an older runtime cannot receive newer content,
and future/incompatible content is refused. Rollback preserves learner workspace bytes;
an older app can refuse newer local data without deleting it.

## Runtime isolation and build identity

The build emits `NIBLI_APP_REVISION` and `NIBLI_RUNTIME_ID`. Clean runtime identity
combines the app and engine Git revisions. Dirty development builds add a deterministic
digest of nonignored source files; generated output and test evidence do not change it.

An exact Git engine pin is required by default. `NIBLI_ALLOW_LOCAL_ENGINE=1` permits
an explicitly unreleasable integration build with `local-<HEAD>-source-<digest>`
engine identity derived from the actual worktree bytes. Catalog/runtime/pack/build
metadata all mark it `development: true`; it is never represented as a clean commit.
The model receives that identity through `NIBLI_BUILD_ENGINE_REVISION`. A release
bundle check rejects development snapshots and requires every engine package to be
pinned to an exact Git source. This opt-in is only for pre-release UI verification.

Runtime assets live under `/nibli-learn/runtime/<identity>/`; declarative content lives
under `/nibli-learn/packs/<id>/<version>/`. The entry page, generated JavaScript, styles,
fonts and worker URLs remain bound to their runtime namespace. Stable aliases remain
available for development and compatibility. Root navigation under service-worker control
serves the explicitly active runtime; already open pages keep their original versioned
asset URLs and worker. Activation offers a reload instead of silently replacing a live
page's engine. Retained runtime caches are not deleted during activation or rollback.

The service worker only serves verified installation records, never unpublished staging
caches, and verifies cached bytes before serving them. It does not broaden its scope or
cache unrelated origins. An offline missing version returns an explicit unavailable
response rather than substituting another engine version.

Platform browser tests use real IndexedDB, CacheStorage, service workers and network-offline
reloads on an isolated fixture origin. Their miniature runtime identifies version binding;
actual reasoning and complete learning journeys are tested against the built WASM app
separately. Quota rejection is exercised with a controlled browser storage fault, without
claiming a physical-device quota stress measurement.

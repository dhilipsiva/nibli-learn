# Versioned content and local data

The [content manifest](../content/manifest.json) identifies nineteen available lessons,
116 stable activities, and the planned specialist studios. Content version 4
requires application schema 4; the installer also retains support for paired schema-2/3
runtimes and content. The registry and references are original learning
material; `just check-content` verifies their IDs, versions, objectives and links.
The build manifest binds these bytes to the exact application and engine inputs.

The authoritative serialized types are in `crates/model/src/`. Assertion IDs and
revisions cross the browser boundary as decimal strings; an assertion identity also
includes its session epoch. A reset may reuse the numeric part.

| Document | Version boundary | Contents and behavior |
| --- | --- | --- |
| Course progress | `schema: 2`, course, lesson and objective versions | Predictions, learner-authored reflection notes, checks, hints, self-review and run identity metadata. No source statements, proofs or execution transcripts. Schema-1 flags migrate to the seven original version-1 objectives without inventing run evidence. |
| Local project | `schema: 2`, `kind: "nibli-project"`, engine/content versions | Local project name/identity, profile, assertion source labels and recovery image, unfinished source-review draft and review-cost assumptions. External imports require detached engine validation before preview and explicit application. |
| Workspace | `schema: 2` | Versioned progress, up to 64 projects, active project, preferences, up to 16 supplied local-lab reports and bounded aborted-attempt metadata. Stored inside a separate transactional revision envelope. |
| Local lab report | `schema: 1`, `kind: "nibli-local-lab-report"` | Lab/engine/kit identity, source inventory, environment, artifact/log hashes and supplied observations. Parsing is bounded to 2 MiB. Compatible reports require successful process statuses and every required observation; this does not authenticate the supplied execution. |
| Operation run | `schema: 2`, pinned engine/content identity | Request, generation, epoch/revision, actual profile, operation-specific result, optional actual proof and a separate execution record. Hypothetical scope identifies its original source epoch/revision. |
| Original proof record | `schema: 1`, `kind: "proof-record"` | The original seven-step Boolean run export, preserving its original claim, outcome, Why, proof and actual execution record. |
| Execution record | Engine `schema_version` and `engine_version` | Bounded actual artifacts/events, profile, recording status and result/error. Inspection getters have no recording; a missing record does not mean an empty completed execution. |

Changed objective versions keep old evidence as history and require their own new
checks. Imported progress is learning history, not an authenticated certificate.
Self-review remains explicitly different from automated checks of actual engine
results. Restoring a project or viewing an archive never grants completion.

Browser saves use IndexedDB transactions with compare-and-swap revisions. The original
schema-1 localStorage bytes are retained during migration. Explicit import saves retain
a pre-import checkpoint; undo restores it under a fresh storage revision. Stale tabs,
quota failures and unavailable storage retain the current candidate for export instead
of silently overwriting newer saved work. See [platform contracts](platform.md).

Completed recordings remain in memory and are exported explicitly. Persistent progress
stores only bounded run identity metadata. An aborted attempt has its claim, settings,
epoch/revision and `aborted` status, with no completed outcome or proof. Hard worker
replacement may capture no events.

Parsing a project envelope does not validate the engine recovery image. The worker
replays and checks that image in a detached candidate: source strings, compiled roots,
guards, compute registry, ID frontier and profile must agree. Presentation assertions
are not authoritative knowledge. Imported projects receive new local identities;
reopening an existing saved project retains its committed identity.

Worker requests bind the expected epoch and input revision. Phase notices report actual
stage entry; only a final response acknowledges a request. Every successful reply's
`session` remains the acknowledged main session, including detached validation and
hypothetical requests. Candidate images are returned in separate fields. Replies from
an earlier worker generation cannot replace the current session.

Find, count and aggregate return definitive collections only after complete enumeration.
Unavailable collections, failed operations and Boolean UNKNOWN are distinct results.
A candidate result cannot become a main result: applying its reviewed source delta uses
fresh atomic main-session admission, checks the original base revision and reruns the
main operation. Candidate assertion IDs and profile settings are not transplanted.

## Verified offline pack contract

The [pack schema](../content/offline-pack.schema.json) defines the declarative content
inventory for application schemas 2, 3 and 4. Schema 3 includes the local-lab catalog;
its source-kit download belongs to the matching runtime inventory. The first-party catalog separately pins runtime
and content manifests by SHA-256; payload inventories exclude their own manifests.
Content packs contain lessons, references and permitted images, never executable
application, worker, JavaScript, WASM or font replacements.

The browser installer downloads every declared asset into an unpublished cache,
checks exact size/hash and compatible identities, and publishes a verified installation
record atomically. Downloading is separate from activation. The UI validates the
installed course and references using the model, then an explicit learner action
commits a compare-and-swap active pointer. Cancellation, missing/corrupt files,
quota failure and interrupted download preserve the prior active installation.
Changed payload bytes at an already installed content version are rejected.

Runtime assets use versioned URLs. An already open page keeps its engine/assets;
new navigation uses the explicitly activated version. Retained installations support
rollback without deleting user projects. An installation is reported installed only
when its active inventory remains complete and hash-valid. Physical device eviction
can remove browser storage, so status and activation verify the actual cache.

See [platform contracts](platform.md) for storage transaction semantics, byte limits,
API shapes, runtime isolation and browser verification scope. The isolated installer
suite exercises real IndexedDB, CacheStorage and service workers with small runtime
fixtures; a full companion offline journey separately verifies the built reasoning
application. External arbitrary pack import and publisher signatures are outside this
first-party catalog contract.

## Source review and supplied evidence

Preparation computes canonical KR, deterministic backtranslation and a real detached
admission check. A worker-held ticket binds exact source/draft bytes, engine, profile
and main epoch/revision. Explicit human confirmation and a fresh main admission are
required. Editing, intervening changes, replay or worker restart cannot reuse approval.
Tickets and confirmation flags are not saved. Retained assertion provenance records
the source and hashes as learner-attested history, not source authenticity.

Review activities retain bounded milestone metadata separately from engine runs.
Reference examples distinguish KR compilation from Rust APIs, WIT, data and local
commands. Local report import creates no browser operation, proof or source assertion.
Failed or incompatible reports remain inspectable but do not satisfy a lab requirement.
Optional model keys and replies are excluded from all these serializable documents.


Build uses paired content/application schema 4. It preserves all F/U/I lesson and
activity identities and their versions, adds B1–B5 and six local experiment types,
and supplies views for captured compiler values. Readers retain schema-2/3 support;
progress/project/operation schemas remain 2. Imported lab reports keep their original
kit digest and do not count as matching evidence after the kit changes.

# Versioned content and local data

The [content manifest](../content/manifest.json) identifies the one available lesson,
its seven stable objective IDs, and the planned public learning groups. A planned
entry is not delivered content. Content version 1 requires application schema 1.
The build manifest records the content version and SHA-256 alongside the pinned
engine sources and all distributed file hashes. `just check-content` checks the
manifest against the lesson's actual titles and objectives and model versions.

The authoritative serialized Rust types are in `crates/model/src/lib.rs`. JSON
identities crossing the browser boundary are strings; a numeric assertion ID is
qualified by its session epoch. Resetting a session may reuse the numeric ID.

| File | Version boundary | Includes | Import behavior in Phase 1 |
| --- | --- | --- | --- |
| Learning progress | `schema`, `lesson`, `lesson_version` | Current step and seven step records: predictions, checked state, hints and authored reasoning visibility | Validate and preview first; explicit replacement retains accepted knowledge |
| Current project | `schema: 1`, `kind: "nibli-project"` | Session epoch, revision, settings, assertions, source labels and validated engine recovery image | Export only; external project import is not enabled |
| Proof record | `schema: 1`, `kind: "proof-record"` | Run identity, original claim, epoch, revision, settings, outcome, Why, proof, proof text and separate actual execution record | Export only |
| Execution record | Engine `schema_version`, `engine_version` | Recording options/status, profile, actual artifacts/events, snapshots, result or error | Export only; replay reads the record already returned by the worker |

Learning progress contains no source statements or execution recording. Predictions
are learning evidence; they are not engine outputs. A checked self-review is not
an automated grade of the learner's prose. Incompatible progress is rejected before
replacement. Unreadable browser data is retained for export; the app does not silently
overwrite it with a new default state.

The local browser envelope uses schema 1. It stores progress, the last committed
session, theme, independent-support exercise stage and aborted attempt metadata.
Completed run recordings remain in memory for the current page session and can be
exported explicitly; they are not implicitly persisted. An aborted attempt records
the attempted claim, settings, session epoch and revision with status `aborted`.
It has no completed outcome or proof. Hard worker replacement may capture no events.

Engine recovery validates a candidate image before replacing the current session.
The caller's presentation assertion list is not authoritative knowledge. The engine
snapshot supplies the accepted roots, admission guards, ID frontier and profile;
source labels must agree with the recovered assertion records.

Worker requests bind both the expected epoch and input revision. Informational phase
notices identify actual engine stage entry; only a final response acknowledges a
request and its session. A phase notice is neither a completed outcome nor a replay
record. Replies from an earlier worker generation cannot replace the current session.

## Offline pack contract: reserved, not implemented

Phase 1 serves a bundled static app and bundled lesson. There is no service worker,
download manager, external pack importer or offline activation guarantee. A browser
may cache files, but that does not constitute a verified offline installation.

The [pack schema](../content/offline-pack.schema.json) defines version 1 metadata
for a future installer. This schema is a contract artifact, not a feature switch.
An archive has one `pack.json`, a declared content manifest and only declared payload files.
It supplies a stable pack ID, monotonic integer version, compatible application
schema and engine commit, and SHA-256/byte count for every payload member. `pack.json`
is excluded from its own file table; a distribution catalog supplies its expected
hash, avoiding a self-referential digest. The declared content manifest must appear
in the payload file table. No application,
worker, JavaScript, WASM or font code can be replaced by a content pack.

Future activation must implement this sequence before the UI claims offline support:

1. Download into a staging namespace. Record the previous active version and never
   modify its files during download, validation or cancellation.
2. Reject unsupported schemas/engine compatibility, repeated paths, absolute paths,
   traversal, symlinks, undeclared files, unsupported media types, and configured
   storage/file-count/size limits. Treat all content as untrusted input.
3. Verify every exact byte count and SHA-256, then validate the content manifest,
   lesson IDs, references and original exercise schemas. Do not execute pack code.
4. Commit one active-version pointer only after the complete staged pack validates.
   Interrupted or failed activation retains the previous complete active version.
5. Verify an offline launch and all declared assets before presenting an installed
   state. Garbage collection must never remove the active version or user projects.

Hash verification detects byte changes; it does not authenticate an unknown author.
Publisher signatures, distribution trust, quotas and migration rules must be fixed
before enabling imports. Progress references exact lesson versions; incompatible
versions require an explicit, validated migration and preserve the original export.

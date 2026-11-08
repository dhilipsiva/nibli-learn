# Local integration labs

These original experiments connect the companion to the pinned native engine and
WASI component. They exercise real engine calls and a real Wasmtime host. A report
imported into the companion is a learner-supplied observation, not authenticated
proof that the lab ran. Keep the raw logs with the JSON report.

Currently verified platform: Linux x86_64, Rust 1.96.1, Python 3.13, and the engine's
locked Wasmtime 49.0.5. Other platforms are unverified. Native measurements use
Linux `/proc` for RSS and report null if unavailable. Start from a new output folder
for every experiment; the runner refuses to overwrite an earlier result.

From a companion checkout, enter `nix develop`. First builds fetch public locked
Rust/Nix dependencies and need network access and disk space. No model, key or
account is needed. The experiments bind temporary TCP servers to loopback only.

```sh
python3 labs/run.py compute --output ../nibli-compute-first
python3 labs/run.py ontology --output ../nibli-ontology-first
python3 labs/run.py measurement --output ../nibli-measurement-first
python3 labs/run.py scoped-application --output ../nibli-scoped-first
```

For WASI, prepare a separate clean engine checkout. The runner checks the exact
revision, builds the component and host using that checkout's pinned Nix shell,
and verifies that tracked source files stay clean. The default build is debug;
its fuel use must not be compared directly with an optimized build.

```sh
git clone https://github.com/dhilipsiva/nibli ../nibli-lab-engine
git -C ../nibli-lab-engine checkout 0440db4233f8a8cffbf81be9654a5e54c1f9c836
python3 labs/run.py wasi-runtime --engine-root ../nibli-lab-engine --output ../nibli-wasi-first
```

For a downloaded kit without the companion checkout, put the `labs` directory
beside the companion's `flake.nix` and `flake.lock` (included in the download).
Run the commands from that directory inside `nix develop`. `target/learning-labs`
contains native build outputs. Only your selected output directory receives the
experiment's journals and logs. Do not run against an application database.

## What to inspect and repair

| Lab | Experiment and expected distinction | Repair to investigate |
| --- | --- | --- |
| `compute` | A repeated external check dispatches once within a proof-producing query, then redispatches on a new query. FALSE is a backend answer; a missing backend, malformed JSON, wrong argument type or timeout is unavailable. Find/count/aggregate do not dispatch. | Register existing vocabulary before storing it; supply typed arguments and a bounded host; rebind callbacks after restore. Explicitly review any evidence you want to persist. |
| `ontology` | The exportable two-place fact fragment round-trips. Distinct IRIs can collapse onto one local name. A refused incremental import can leave earlier rows in its candidate. Flavor and ontology declarations cannot be reconstructed from a fact-only export. | Check namespace mappings before import, validate the supported syntax, stage in a detached candidate, and discard the whole candidate after refusal. Inspect every export refusal. |
| `measurement` | Three distinct input shapes run three times. Initialization, compile-only, text admission, non-proof query, proof query and proof serialization are measured separately. | State the measured boundary; compare distributions from equivalent builds and workloads. Report initialization and retained allocator memory, rather than calling all time "reasoning" or all RSS "allocation". |
| `scoped-application` | Review from CaseB cannot satisfy CaseA. Correct review supports the conclusion; direct assertion of the protected conclusion fails. Reopening preserves IDs and a withdrawal remains effective after another reopen. A native authorizer isolates temporary context and allows only TRUE. | Join on the same case identity, admit source records through the correct policy, and maintain/retract actual source IDs. |
| `wasi-runtime` | Inspect actual imports/exports. Exhaust fuel and memory separately, raise each host limit, recover the journal, and rerun the original query. Reopen a journal with a deleted middle ID. The scoped application's before/after decisions match native execution. A loopback backend supplies actual compute answers without creating durable assertions. | Locate the failing layer; preserve the last committed journal; rebuild a trapped component before replay; choose a justified host budget. A higher depth is not a fuel or memory repair. |

The compute lab deliberately compares two representations of a non-finite numeric
operand. The compiler-shaped event group reports `Unknown(NonFinite)`. A caller-built
flat compute node can reach the host callback; this lab's callback refuses it, which
reports `Unknown(BackendUnavailable)`. Raw IR callers must uphold their own ingress
and host contracts. A TRUE returned by a compute backend is trusted for that query;
this protocol does not establish identity, freshness or real-world accuracy.

The small ontology reader is not a full Turtle validator. The lab shows that `[]`
can be accepted as a bare IRI token rather than a blank node. Its application
preflight rejects that token and detects namespace collisions. This limited check
is not a general RDF validator. A production importer should use a conforming RDF
parser and an explicit supported-fragment mapping, with regression examples for
accepted and refused syntax.

Allocation counts come from a Rust global-allocator wrapper; its overhead is part
of the experiment. Requested bytes count the new requested size on reallocation.
They are neither live bytes nor peak memory. RSS includes process runtime and
allocator-retained pages. Repetitions share one process, so later runs are warm.
These observations support diagnosis, not a claim that one reasoner is faster.

The WASI component has an external-compute import plus WASI IO, CLI and insecure
random seed imports. Its guest interface has no filesystem, clock, socket or HTTP
import in this pin. The native host still has its own capabilities. Its memory cap
limits guest growth, not process RSS, and fuel counts WASM work, not elapsed time.
The experiment does not exercise the exported authorizer interface.

## Evidence and return to the course

`report.json` records the engine pin, a digest of the kit sources, platform/tool
versions, artifact hashes, observed checks and hashes of raw stdout/stderr logs.
A failed experiment exits nonzero and produces a failed report when possible.
Inspect the error before rerunning; do not change a failed check into a pass.
Journals are local experimental data. Reports and logs may contain local paths;
review them before sharing.

Return to the matching Integrate Applications activity and import the report into
the local-lab panel. The panel must label it as supplied evidence, retain its
identity and failure details, and never turn it into a browser-engine run. Answer
the activity's diagnosis and self-review prompts using the actual observations.

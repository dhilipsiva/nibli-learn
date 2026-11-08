# Nibli Learn

An interactive learning environment for developers learning symbolic and
neuro-symbolic reasoning. Ten original units cover shared foundations and using
a reasoner through 44 activities, with executable exercises backed by Nibli's
real browser engine.

The learning loop is predict, change evidence, execute, inspect and repair.
Execution recordings describe actual operations; proofs describe derivations.
Neither a FALSE engine result nor an admission refusal implies an incorrect
learner answer.

## Development

Enter the pinned environment with `nix develop`. It provides Rust 1.96.1,
Dioxus CLI 0.7.3, WASM tooling, Node and, on Linux, Chromium.

```sh
just dependencies
just ci-native
just build
just check-bundle
just test-browser
just test-platform
just serve
```

Open `http://127.0.0.1:8080/nibli-learn/`. The application uses hash routes, so
the static server does not need route-rewrite rules. Serve `dist/`, not the
application subdirectory: worker, font and stylesheet paths share the
`/nibli-learn/` mount.

`just ci` runs the native, WASM, bundle and real-browser checks. Browser evidence
is written to ignored `test-results/`. `BROWSER_EXECUTABLE` can select a compatible
Chromium installation; without it, install the pinned Playwright browser with
`npx playwright install chromium`.

## Build and data boundaries

The release bundle contains the Dioxus application, a dedicated WASM reasoning
worker, original public assets and a hash manifest. Engine dependencies must be
pinned to reachable Git commits for a release; local path dependencies are a
development convenience and fail the release-input check.

Reasoning runs locally in the browser. Lessons require no account, analytics
or model key. Run history preserves input revision and session
identity. A cancelled run has no completed logical outcome; recovery restores
the last committed knowledge.

The versioned [content manifest](content/manifest.json) covers foundations F1–F6
and Use units U1–U4. Later integration, engine-building and studio tracks remain
planned. [Data contracts](docs/data-contracts.md) describe versioned progress,
projects, read-only run archives and verified offline installation. The Local Data
page provides import previews, backup/undo and recovery from storage conflicts.

Offline installation stages and verifies the matching runtime and original course
before explicit activation. It preserves the previous active version on failure.
The [platform contracts](docs/platform.md) explain version isolation and browser
storage limits. The five-person learning pilot remains a separate Phase 2 gate;
automated checks do not establish learner comprehension.

The browser test suite exercises actual WASM and worker messages. Prototype
fixtures are not an implementation substitute. CI uploads a verified artifact;
it does not deploy a website or publish a release.

## Licenses

Code is available under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
Original educational content uses [CC BY 4.0](CONTENT-LICENSE.md).
Bundled fonts and other third-party assets retain their accompanying notices.

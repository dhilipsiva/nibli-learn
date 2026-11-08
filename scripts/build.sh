#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# Capture first so a rejected local engine cannot be hidden by process substitution.
NIBLI_IDENTITIES_TEXT="$(python3 scripts/runtime_identity.py)"
mapfile -t NIBLI_BUILD_IDENTITIES <<< "$NIBLI_IDENTITIES_TEXT"
export NIBLI_APP_REVISION="${NIBLI_BUILD_IDENTITIES[0]}"
export NIBLI_RUNTIME_ID="${NIBLI_BUILD_IDENTITIES[1]}"
export NIBLI_BUILD_ENGINE_REVISION="${NIBLI_BUILD_IDENTITIES[2]}"
python3 scripts/check_content.py
python3 scripts/lab_catalog.py --check
for tool in cargo dx wasm-pack python3; do
  command -v "$tool" >/dev/null || { echo "Missing $tool; enter nix develop first." >&2; exit 1; }
done
dx --version | grep -Eq 'dioxus 0\.7\.3([[:space:]]|$)' || {
  echo 'The build requires Dioxus CLI 0.7.3, matching the app dependency.' >&2; exit 1;
}
# Preserve caller flags while remapping Rust source/panic locations in both WASM
# builds. Removing debug symbols alone does not remove these embedded paths.
export CARGO_ENCODED_RUSTFLAGS="$(python3 scripts/build_paths.py rustflags)"
STAGE="$ROOT/target/nibli-learn-stage"
WORKER_PKG="$ROOT/target/nibli-learn-worker-pkg"
UI_OUT="$ROOT/target/dx/nibli-learn-app/release/web/public"
rm -rf "$STAGE" "$WORKER_PKG"
mkdir -p "$STAGE/worker"
wasm-pack build crates/worker --release --target web --out-dir "$WORKER_PKG" --out-name nibli_learn_worker -- --locked
# Dioxus retains earlier hashed assets between builds; stage only this build.
rm -rf "$UI_OUT"
(
  cd crates/app
  dx build --release --locked --platform web --base-path nibli-learn --debug-symbols false
)
test -f "$UI_OUT/index.html" || { echo "Missing Dioxus bundle: $UI_OUT" >&2; exit 1; }
cp -R "$UI_OUT/." "$STAGE/"
python3 scripts/build_paths.py sanitize-js "$STAGE" "$UI_OUT/wasm/nibli-learn-app.js"
mkdir -p "$STAGE/worker/pkg"
cp crates/worker/bootstrap.js "$STAGE/worker/bootstrap.js"
cp "$WORKER_PKG/nibli_learn_worker.js" "$WORKER_PKG/nibli_learn_worker_bg.wasm" "$STAGE/worker/pkg/"
cp -R assets "$STAGE/"
python3 scripts/package_labs.py "$STAGE/assets/nibli-labs-v1.tar.gz"
cp -R content "$STAGE/"
mkdir -p "$STAGE/licenses"
cp LICENSE-MIT LICENSE-APACHE CONTENT-LICENSE.md "$STAGE/licenses/"
mkdir -p "$STAGE/platform"
cp crates/client/js/platform.js "$STAGE/platform/platform.js"
cp crates/client/js/tutor.js "$STAGE/platform/tutor.js"
cp scripts/service-worker.js "$STAGE/service-worker.js"
python3 scripts/build_offline.py "$STAGE"
python3 scripts/build_manifest.py "$STAGE"
python3 scripts/check_bundle.py "$STAGE"
mkdir -p dist
rm -rf dist/nibli-learn
mv "$STAGE" dist/nibli-learn
echo 'Built dist/nibli-learn; serve dist/ to preserve the /nibli-learn/ mount.'

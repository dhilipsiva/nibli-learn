#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
python3 scripts/check_content.py
for tool in cargo dx wasm-pack python3; do
  command -v "$tool" >/dev/null || { echo "Missing $tool; enter nix develop first." >&2; exit 1; }
done
dx --version | grep -Eq 'dioxus 0\.7\.3([[:space:]]|$)' || {
  echo 'The build requires Dioxus CLI 0.7.3, matching the app dependency.' >&2; exit 1;
}
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
mkdir -p "$STAGE/worker/pkg"
cp crates/worker/bootstrap.js "$STAGE/worker/bootstrap.js"
cp "$WORKER_PKG/nibli_learn_worker.js" "$WORKER_PKG/nibli_learn_worker_bg.wasm" "$STAGE/worker/pkg/"
cp -R assets "$STAGE/"
cp -R content "$STAGE/"
mkdir -p "$STAGE/licenses"
cp LICENSE-MIT LICENSE-APACHE CONTENT-LICENSE.md "$STAGE/licenses/"
python3 scripts/build_manifest.py "$STAGE"
python3 scripts/check_bundle.py "$STAGE"
mkdir -p dist
rm -rf dist/nibli-learn
mv "$STAGE" dist/nibli-learn
echo 'Built dist/nibli-learn; serve dist/ to preserve the /nibli-learn/ mount.'

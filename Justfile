set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --locked --workspace --all-targets -- -D warnings

test:
    cargo test --locked --workspace --lib
    cargo test --locked -p nibli-learn-app --bins

check-web:
    cargo check --locked --workspace --target wasm32-unknown-unknown

check-content:
    python3 scripts/check_content.py

dependencies:
    npm ci --ignore-scripts

build:
    bash scripts/build.sh

check-bundle:
    python3 scripts/check_bundle.py dist/nibli-learn

test-browser:
    npm run test:browser

ci-native: fmt-check clippy test check-web check-content

ci-web: build check-bundle test-browser

ci: ci-native ci-web

serve PORT="8080":
    python3 -m http.server {{PORT}} --bind 127.0.0.1 --directory dist

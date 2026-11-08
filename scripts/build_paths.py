#!/usr/bin/env python3
"""Keep machine-specific builder paths out of the distributed browser files."""
import json
import os
from pathlib import Path
import re
import sys


def builder_prefixes():
    """Include both logical and resolved locations, including custom Cargo homes."""
    locations = [Path.home(), Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")),
                 Path(__file__).absolute().parent.parent]
    return {str(path.absolute()) for path in locations} | {str(path.resolve()) for path in locations}


def rustflags():
    # Cargo gives encoded flags precedence over its whitespace-separated form.
    encoded = os.environ.get("CARGO_ENCODED_RUSTFLAGS")
    flags = (encoded.split("\x1f") if encoded else []) if encoded is not None \
        else os.environ.get("RUSTFLAGS", "").split()
    home = Path.home()
    cargo_home = Path(os.environ.get("CARGO_HOME", home / ".cargo"))
    checkout = Path(__file__).absolute().parent.parent
    # rustc uses the last matching prefix. Keep the specific source roots last.
    mappings = [(home, "/build-home"), (cargo_home, "/cargo"), (checkout, "/nibli-learn")]
    for source, destination in mappings:
        for prefix in sorted({str(source.absolute()), str(source.resolve())}):
            flags.append(f"--remap-path-prefix={prefix}={destination}")
    return "\x1f".join(flags)


def sanitize_generated_js(stage, original_js):
    """Undo Dioxus 0.7.3's filesystem URL substitution in module metadata only."""
    generated = list((stage / "assets").glob("nibli-learn-app-dxh*.js"))
    assert len(generated) == 1, "Expected exactly one generated Dioxus JavaScript module"
    path = generated[0]
    source = path.read_text()
    # This is executable module metadata, not a comment. Restore the browser's
    # module URL; do not replace arbitrary source strings or relative imports.
    pattern = re.compile(r"(\bconst importMeta\s*=\s*\{\s*url\s*:\s*)"
                         + re.escape(json.dumps(str(original_js)))
                         + r"(?=\s*,\s*main\s*:\s*import\.meta\.main\s*\})")
    source, count = pattern.subn(r"\g<1>import.meta.url", source)
    assert count == 1, "Expected Dioxus filesystem module URL was not found exactly once"
    path.write_text(source)


def assert_public_bytes(data, name):
    # Check WASM as bytes too: panic locations survive stripping debug sections.
    assert not re.search(rb"/(?:home|Users|root|tmp|private/tmp)/|[A-Za-z]:[\\/]+Users[\\/]+", data), \
        f"Private builder path in bundle: {name}"
    for prefix in builder_prefixes():
        assert (prefix.rstrip("/") + "/").encode() not in data, \
            f"Current builder path in bundle: {name}"
    assert b"nibli/book" not in data and b"session-inventory.json" not in data, \
        f"Private source marker in bundle: {name}"


if __name__ == "__main__":
    if sys.argv[1:] == ["rustflags"]:
        print(rustflags())
    elif len(sys.argv) == 4 and sys.argv[1] == "sanitize-js":
        sanitize_generated_js(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        raise SystemExit("Usage: build_paths.py rustflags | sanitize-js STAGE ORIGINAL_JS")

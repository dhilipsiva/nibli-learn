#!/usr/bin/env python3
"""Stage immutable runtime/content namespaces and a first-party pinned catalog."""
import hashlib
import json
import mimetypes
import os
from pathlib import Path
import re
import shutil
import sys
import tomllib

bundle = Path(sys.argv[1])
root = Path(__file__).resolve().parent.parent
runtime_id = os.environ["NIBLI_RUNTIME_ID"]
app_revision = os.environ["NIBLI_APP_REVISION"]
assert re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9._-]{0,191}", runtime_id)
packages = tomllib.loads((root / "Cargo.lock").read_text())["package"]
engine = next(p for p in packages if p["name"] == "nibli-session")
engine_revision = os.environ["NIBLI_BUILD_ENGINE_REVISION"]
development = engine_revision.startswith("local-")
assert not development or os.environ.get("NIBLI_ALLOW_LOCAL_ENGINE") == "1"
assert development or engine_revision == engine["source"].rsplit("#", 1)[-1]
content = json.loads((bundle / "content/manifest.json").read_text())
assert content["content_version"] == 3 and content["minimum_app_schema"] == 3
for name in ["course.json", "references.json"]:
    assert json.loads((bundle / "content" / name).read_text())["schema"] == 3

def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()

def inventory(directory):
    result = []
    for path in sorted(directory.rglob("*")):
        if not path.is_file():
            continue
        assert not path.is_symlink(), path
        data = path.read_bytes()
        media = {".wasm": "application/wasm", ".js": "text/javascript", ".json": "application/json", ".md": "text/markdown", ".woff2": "font/woff2"}.get(path.suffix)
        result.append({"path": path.relative_to(directory).as_posix(), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data), "media_type": media or mimetypes.guess_type(path.name)[0] or "application/octet-stream"})
    return result

runtime_base = f"/nibli-learn/runtime/{runtime_id}/"
runtime_dir = bundle / "runtime" / runtime_id
runtime_dir.mkdir(parents=True)
for source in list(bundle.iterdir()):
    name = source.name
    if name in {"runtime", "packs", "content", "offline", "build-manifest.json"}:
        continue
    if source.is_dir():
        shutil.copytree(source, runtime_dir / name)
    else:
        shutil.copy2(source, runtime_dir / name)
# The application computes its own versioned worker/CSS paths. Rewrite generated
# entry assets and font URLs so HTML/JS/CSS loaded before Rust also stay bound.
for path in runtime_dir.rglob("*"):
    if path.suffix in {".html", ".js", ".css"}:
        value = path.read_text()
        for prefix in ["assets/", "worker/"]:
            value = value.replace(f"/nibli-learn/{prefix}", f"{runtime_base}{prefix}")
        path.write_text(value)

pack_id = "nibli-core"
version = content["content_version"]
pack_base = f"/nibli-learn/packs/{pack_id}/{version}/"
pack_dir = bundle / "packs" / pack_id / str(version)
shutil.copytree(bundle / "content", pack_dir)
content_files = inventory(pack_dir)
digest_rows = [{key: file[key] for key in ("path", "sha256", "bytes")} for file in content_files]
content_digest = hashlib.sha256(json.dumps(digest_rows, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
runtime_manifest = {"schema": 1, "kind": "nibli-runtime", "id": runtime_id, "app_schema": 3, "app_revision": app_revision,
                    "engine_revision": engine_revision, "development": development, "base_path": runtime_base, "files": inventory(runtime_dir)}
pack_manifest = {"schema": 1, "kind": "nibli-content-pack", "id": pack_id, "version": version,
                 "content_version": version, "app_schema": 3, "engine_revision": engine_revision, "development": development,
                 "base_path": pack_base, "content_manifest": "manifest.json", "content_digest": content_digest, "files": content_files}
offline = bundle / "offline"
offline.mkdir()
runtime_name = f"runtime-{runtime_id}.json"
pack_name = f"pack-{pack_id}-{version}-{engine_revision}.json"
runtime_bytes = encoded(runtime_manifest)
pack_bytes = encoded(pack_manifest)
(offline / runtime_name).write_bytes(runtime_bytes)
(offline / pack_name).write_bytes(pack_bytes)
catalog = {"schema": 1, "kind": "nibli-offline-catalog", "entries": [{
    "id": runtime_id, "title": "Foundations, Use and Integrate Applications", "app_schema": 3, "development": development,
    "runtime": {"url": f"/nibli-learn/offline/{runtime_name}", "sha256": hashlib.sha256(runtime_bytes).hexdigest()},
    "content": {"url": f"/nibli-learn/offline/{pack_name}", "sha256": hashlib.sha256(pack_bytes).hexdigest()},
}]}
(offline / "catalog.json").write_bytes(encoded(catalog))
print(f"Staged runtime {runtime_id}, content {pack_id}@{version}, {len(runtime_manifest['files'])} runtime and {len(content_files)} content files")

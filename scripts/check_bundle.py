#!/usr/bin/env python3
"""Validate the actual shipping mount, worker paths, manifest and content boundary."""
import hashlib
import json
from pathlib import Path
import re
import sys
from build_paths import assert_public_bytes

root = Path(sys.argv[1])
required = ["index.html", "worker/bootstrap.js", "worker/pkg/nibli_learn_worker.js",
            "worker/pkg/nibli_learn_worker_bg.wasm", "assets/app.css", "assets/fonts.css",
            "licenses/LICENSE-MIT", "licenses/LICENSE-APACHE", "licenses/CONTENT-LICENSE.md",
            "content/manifest.json", "content/offline-pack.schema.json",
            "content/course.json", "content/references.json", "offline/catalog.json", "service-worker.js", "platform/platform.js",
            "build-manifest.json"]
for name in required:
    assert (root / name).is_file(), f"Missing bundled file: {name}"
manifest = json.loads((root / "build-manifest.json").read_text())
assert_public_bytes((root / "build-manifest.json").read_bytes(), "build-manifest.json")
assert manifest["schema"] == 1 and manifest["base_path"] == "/nibli-learn/"
assert {"nibli-session", "nibli-types", "nibli-protocol", "nibli-render", "nibli-formalize", "nibli-kr"} <= set(manifest["engine"])
content_path = root / "content/manifest.json"
content = json.loads(content_path.read_text())
assert manifest["content"] == {"manifest": "content/manifest.json", "schema": content["schema"],
                               "version": content["content_version"],
                               "sha256": hashlib.sha256(content_path.read_bytes()).hexdigest()}
if "--release" in sys.argv:
    assert manifest.get("development") is False, "Development engine snapshots cannot be released"
    assert manifest["app_revision"] != "uncommitted-development"
    assert not manifest["app_dirty"], "Release bundle must be built from a clean app commit"
    assert all(p["source"].rsplit("#", 1)[-1] == manifest["engine_identity"] for p in manifest["engine"].values()), "Engine package revisions must agree"
    assert all(p["source"].startswith("git+") and re.search(r"#[a-f0-9]{40}$", p["source"])
               for p in manifest["engine"].values()), "Release engine inputs must be pinned Git commits"
actual = {p.relative_to(root).as_posix() for p in root.rglob("*")
          if p.is_file() and p.name != "build-manifest.json"}
assert actual == set(manifest["files"]), "Manifest does not cover exactly the distributed files"
for name, expected in manifest["files"].items():
    path = root / name
    assert not path.is_symlink(), f"Symlink in bundle: {name}"
    data = path.read_bytes()
    assert len(data) == expected["bytes"] and hashlib.sha256(data).hexdigest() == expected["sha256"], name
    assert_public_bytes(data, name)
    assert not any(x in name.lower() for x in ["claude-design", ".dc.html", "editorial/", "render/shots", "canvas/project"]), name
    if path.suffix in {".html", ".js", ".css", ".json", ".md", ".txt"}:
        text = data.decode("utf-8")
        assert not re.search(r"(?:https?:)?//(?:fonts\.googleapis|fonts\.gstatic)\.com", text), name
        if path.suffix == ".html":
            for ref in re.findall(r'(?:src|href)=["\']([^"\']+)["\']', text):
                if ref.startswith("/") and not ref.startswith("//"):
                    assert ref.startswith("/nibli-learn/"), f"Asset outside application mount: {ref}"
assert "./pkg/nibli_learn_worker.js" in (root / "worker/bootstrap.js").read_text()
assert (root / "worker/pkg/nibli_learn_worker_bg.wasm").read_bytes()[:4] == b"\0asm"
app_js = list(root.glob("assets/nibli-learn-app-dxh*.js"))
app_wasm = list(root.glob("assets/nibli-learn-app_bg-dxh*.wasm"))
assert len(app_js) == len(app_wasm) == 1, "Stale Dioxus assets from an earlier build must not ship"
assert app_js[0].name in (root / "index.html").read_text()
assert app_wasm[0].name in app_js[0].read_text()
catalog = json.loads((root / "offline/catalog.json").read_text())
assert catalog["schema"] == 1 and catalog["kind"] == "nibli-offline-catalog"
assert len(catalog["entries"]) == 1
entry = catalog["entries"][0]
assert entry["id"] == manifest["runtime_id"] and entry["app_schema"] == 2
for kind in ["runtime", "content"]:
    ref = entry[kind]
    assert ref["url"].startswith("/nibli-learn/offline/")
    path = root / ref["url"].removeprefix("/nibli-learn/")
    assert hashlib.sha256(path.read_bytes()).hexdigest() == ref["sha256"]
    part = json.loads(path.read_text())
    assert part["app_schema"] == 2
    assert part["engine_revision"] == manifest["engine_identity"]
    assert part["development"] == entry["development"] == manifest["development"]
    assert part["base_path"].startswith("/nibli-learn/")
    part_root = root / part["base_path"].removeprefix("/nibli-learn/")
    assert {p.relative_to(part_root).as_posix() for p in part_root.rglob("*") if p.is_file()} == {f["path"] for f in part["files"]}
    for file in part["files"]:
        payload = (part_root / file["path"]).read_bytes()
        assert len(payload) == file["bytes"] and hashlib.sha256(payload).hexdigest() == file["sha256"]
    if kind == "runtime":
        index = (part_root / "index.html").read_text()
        assert f'{part["base_path"]}assets/' in index
        assert "/nibli-learn/assets/" not in index
        for css in (part_root / "assets").glob("*.css"):
            assert "/nibli-learn/assets/" not in css.read_text()
    else:
        assert part["content_version"] == content["content_version"] == 2
        rows = [{key: file[key] for key in ("path", "sha256", "bytes")} for file in sorted(part["files"], key=lambda f: f["path"])]
        assert hashlib.sha256(json.dumps(rows, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest() == part["content_digest"]
print(f"Bundle verified: {len(actual)} files, /nibli-learn/ mount, actual worker WASM and matching hashes")

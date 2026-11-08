#!/usr/bin/env python3
"""Validate the actual shipping mount, worker paths, manifest and content boundary."""
import hashlib
import json
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
required = ["index.html", "worker/bootstrap.js", "worker/pkg/nibli_learn_worker.js",
            "worker/pkg/nibli_learn_worker_bg.wasm", "assets/app.css", "assets/fonts.css",
            "licenses/LICENSE-MIT", "licenses/LICENSE-APACHE", "licenses/CONTENT-LICENSE.md",
            "content/manifest.json", "content/offline-pack.schema.json",
            "build-manifest.json"]
for name in required:
    assert (root / name).is_file(), f"Missing bundled file: {name}"
manifest = json.loads((root / "build-manifest.json").read_text())
assert manifest["schema"] == 1 and manifest["base_path"] == "/nibli-learn/"
assert set(manifest["engine"]) == {"nibli-session", "nibli-types", "nibli-protocol", "nibli-render"}
content_path = root / "content/manifest.json"
content = json.loads(content_path.read_text())
assert manifest["content"] == {"manifest": "content/manifest.json", "schema": content["schema"],
                               "version": content["content_version"],
                               "sha256": hashlib.sha256(content_path.read_bytes()).hexdigest()}
if "--release" in sys.argv:
    assert manifest["app_revision"] != "uncommitted-development"
    assert not manifest["app_dirty"], "Release bundle must be built from a clean app commit"
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
    assert not any(x in name.lower() for x in ["claude-design", ".dc.html", "editorial/", "render/shots", "canvas/project"]), name
    if path.suffix in {".html", ".js", ".css", ".json", ".md", ".txt"}:
        text = data.decode("utf-8")
        assert "nibli/book" not in text and "session-inventory.json" not in text, name
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
print(f"Bundle verified: {len(actual)} files, /nibli-learn/ mount, actual worker WASM and matching hashes")

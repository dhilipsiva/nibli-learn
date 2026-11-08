#!/usr/bin/env python3
"""Bind a distributable to its actual engine sources and file bytes."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

bundle = Path(sys.argv[1])
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--format-version", "1"], text=True
))
engines = {}
for package in metadata["packages"]:
    if package["name"] in {"nibli-session", "nibli-types", "nibli-protocol", "nibli-render"}:
        engines[package["name"]] = {
            "version": package["version"],
            "source": package["source"] or "local-development",
        }
try:
    app_root = Path(__file__).resolve().parent.parent
    git_root = Path(subprocess.check_output(
        ["git", "rev-parse", "--show-toplevel"], text=True, stderr=subprocess.DEVNULL
    ).strip()).resolve()
    if git_root != app_root:
        raise ValueError("The application must have its own repository.")
    app_revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL
    ).strip()
    app_dirty = bool(subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=normal"], text=True
    ).strip())
except (subprocess.CalledProcessError, ValueError):
    app_revision = "uncommitted-development"
    app_dirty = True
files = {}
content_path = bundle / "content/manifest.json"
content = json.loads(content_path.read_text())
for path in sorted(bundle.rglob("*")):
    if path.is_file() and path.name != "build-manifest.json":
        files[path.relative_to(bundle).as_posix()] = {
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "bytes": path.stat().st_size,
        }
(bundle / "build-manifest.json").write_text(json.dumps({
    "schema": 1,
    "base_path": "/nibli-learn/",
    "app_revision": app_revision,
    "app_dirty": app_dirty,
    "engine": engines,
    "content": {
        "manifest": "content/manifest.json",
        "schema": content["schema"],
        "version": content["content_version"],
        "sha256": hashlib.sha256(content_path.read_bytes()).hexdigest(),
    },
    "files": files,
}, indent=2) + "\n")

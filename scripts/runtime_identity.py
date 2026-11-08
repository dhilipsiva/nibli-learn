#!/usr/bin/env python3
"""Emit app, runtime and actual engine source identities for a build."""
import hashlib
import os
from pathlib import Path
import subprocess
import tomllib

root = Path(__file__).resolve().parent.parent
generated = {'target', 'dist', 'node_modules', 'test-results', 'playwright-report', 'artifacts'}

def git(directory, *args):
    return subprocess.check_output(['git', *args], cwd=directory).decode().strip()

def source_digest(directory):
    digest = hashlib.sha256()
    names = set(subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=directory).decode().split('\0'))
    for name in sorted(names - {''}):
        if name.split('/', 1)[0] in generated:
            continue
        path = directory / name
        digest.update(name.encode() + b'\0')
        digest.update(path.read_bytes() if path.is_file() else b'<removed>')
        digest.update(b'\0')
    return digest.hexdigest()[:16]

app = git(root, 'rev-parse', 'HEAD')
lock = tomllib.loads((root / 'Cargo.lock').read_text())
engine = next(package for package in lock['package'] if package['name'] == 'nibli-session')
if engine.get('source', '').startswith('git+'):
    revision = engine['source'].rsplit('#', 1)[-1]
else:
    if os.environ.get('NIBLI_ALLOW_LOCAL_ENGINE') != '1':
        raise SystemExit('An exact Git engine pin is required. For an explicitly unreleasable development build, set NIBLI_ALLOW_LOCAL_ENGINE=1.')
    workspace = tomllib.loads((root / 'Cargo.toml').read_text())
    session = workspace['workspace']['dependencies']['nibli-session']
    source = (root / session['path']).resolve()
    engine_root = Path(git(source, 'rev-parse', '--show-toplevel'))
    revision = f"local-{git(engine_root, 'rev-parse', 'HEAD')}-source-{source_digest(engine_root)}"
runtime = f'{app}-{revision}'
if git(root, 'status', '--porcelain', '--untracked-files=normal'):
    runtime += f'-dev-{source_digest(root)}'
print(app)
print(runtime)
print(revision)

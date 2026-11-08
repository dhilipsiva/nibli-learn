#!/usr/bin/env python3
"""Build a deterministic local kit from an explicit public source allowlist."""
import gzip
import hashlib
import io
from pathlib import Path
import sys
import tarfile
PUBLIC_INPUTS = (
    'labs/README.md', 'labs/run.py',
    'labs/fixtures/ontology-fragment.ttl', 'labs/fixtures/scoped-inspection.nibli',
    'labs/native/Cargo.toml', 'labs/native/Cargo.lock',
    'labs/native/src/main.rs', 'labs/native/src/compute.rs',
    'labs/native/src/ontology.rs', 'labs/native/src/measurement.rs',
    'labs/native/src/scoped.rs',
    'flake.nix', 'flake.lock', 'LICENSE-MIT', 'LICENSE-APACHE', 'CONTENT-LICENSE.md',
)

def package(root, out):
    from build_paths import assert_public_bytes
    actual = {p.relative_to(root).as_posix() for p in (root / 'labs').rglob('*')
              if p.is_file() and not any(s in {'target', '__pycache__'} for s in p.relative_to(root / 'labs').parts)}
    assert actual == {name for name in PUBLIC_INPUTS if name.startswith('labs/')}, 'Review changes to the public lab source allowlist.'
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode='w', format=tarfile.PAX_FORMAT) as archive:
        for name in sorted(PUBLIC_INPUTS):
            path = root / name
            assert not path.is_symlink(), f'Symlink is not a public source input: {name}'
            data = path.read_bytes()
            assert_public_bytes(data, name)
            info = tarfile.TarInfo(name)
            info.size = len(data); info.mode = 0o644; info.mtime = 0
            info.uid = info.gid = 0; info.uname = info.gname = ''
            archive.addfile(info, io.BytesIO(data))
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open('wb') as raw:
        with gzip.GzipFile(fileobj=raw, filename='', mtime=0, mode='wb', compresslevel=9) as zipped:
            zipped.write(buffer.getvalue())
    print(f'Packaged {len(PUBLIC_INPUTS)} lab source files; sha256 {hashlib.sha256(out.read_bytes()).hexdigest()}')

if __name__ == '__main__':
    package(Path(__file__).resolve().parent.parent, Path(sys.argv[1]))

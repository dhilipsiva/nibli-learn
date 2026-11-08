#!/usr/bin/env python3
"""Exercise the portable native kit; retain real reports and failures under target/."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
(root / 'target').mkdir(exist_ok=True)
output = Path(tempfile.mkdtemp(prefix='native-lab-check-', dir=root / 'target'))
for lab in ['compute', 'ontology', 'measurement', 'scoped-application']:
    subprocess.run(['python3', 'labs/run.py', lab, '--output', str(output / lab)], cwd=root, check=True)
print('Native lab checks passed. WASI component/host checks require the separate pinned engine checkout.')

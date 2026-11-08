#!/usr/bin/env python3
"""Isolated same-origin browser fixture server; never edits the distributable."""
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import time

root = Path(__file__).resolve().parent.parent
site = root / "test-results/platform-site"
if site.exists():
    shutil.rmtree(site)
app = site / "nibli-learn"
(app / "platform").mkdir(parents=True)
shutil.copy2(root / "crates/client/js/platform.js", app / "platform/platform.js")
shutil.copy2(root / "scripts/service-worker.js", app / "service-worker.js")
(app / "index.html").write_text('<!doctype html><meta charset="utf-8"><title>Platform verification</title><h1>Platform verification</h1>')

class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(site), **kwargs)

    def log_message(self, *_):
        pass

    def do_GET(self):
        # A real pending response lets the interruption test close a downloading
        # tab without mocking fetch or relying on service-worker routing hooks.
        delay = site / 'delay-path.txt'
        if delay.exists() and delay.read_text() == self.path:
            (site / 'delay-started').touch()
            deadline = time.monotonic() + 20
            while delay.exists() and time.monotonic() < deadline:
                time.sleep(0.02)
        try:
            super().do_GET()
        except (BrokenPipeError, ConnectionResetError):
            pass

ThreadingHTTPServer(("127.0.0.1", 4181), Handler).serve_forever()

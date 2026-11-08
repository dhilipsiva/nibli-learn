#!/usr/bin/env python3
"""Check public content identity against the lesson actually compiled into the UI."""
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parent.parent
content = json.loads((root / "content/manifest.json").read_text())
model = (root / "crates/model/src/lib.rs").read_text()
lesson = (root / "crates/app/src/lesson.rs").read_text()
assert content["schema"] == content["content_version"] == content["minimum_app_schema"] == 1
assert content["kind"] == "nibli-learn-content-manifest"
assert content["offline_packs"]["installation"] == "not-implemented"
assert len(content["lessons"]) == 1
available = content["lessons"][0]
assert available["availability"] == "available"
assert available["id"] == re.search(r'LESSON_ID: &str = "([^"]+)"', model)[1]
assert available["version"] == int(re.search(r'LESSON_VERSION: u32 = (\d+)', model)[1])
steps = available["steps"]
assert [step["number"] for step in steps] == list(range(1, 8))
assert len({step["id"] for step in steps}) == 7
assert [step["title"] for step in steps] == re.findall(r'\btitle: "([^"]+)"', lesson)
assert [step["objective"] for step in steps] == re.findall(r'\bobjective: "([^"]+)"', lesson)
assert [unit["id"] for unit in content["foundations"]] == [f"F{i}" for i in range(1, 7)]
assert [unit["lesson"] for unit in content["foundations"] if unit["availability"] == "available"] == [available["id"]]
schema = json.loads((root / "content/offline-pack.schema.json").read_text())
assert schema["properties"]["schema"]["const"] == content["offline_packs"]["contract_schema"]
assert schema["properties"]["kind"]["const"] == "nibli-content-pack"
print("Content verified: one available lesson, seven matching objectives, six public groups, reserved pack schema 1")

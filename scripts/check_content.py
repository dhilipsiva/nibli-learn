#!/usr/bin/env python3
"""Check delivery metadata against the declarative course/reference registry.

Rust model parsing and engine fixture tests validate semantic/executable content.
This check prevents a stale delivery manifest from describing different activities.
"""
import json
from pathlib import Path
import re

root = Path(__file__).resolve().parent.parent
content = json.loads((root / 'content/manifest.json').read_text())
course = json.loads((root / 'content/course.json').read_text())
references = json.loads((root / 'content/references.json').read_text())
assert content['schema'] == content['content_version'] == content['minimum_app_schema'] == 4
assert content['kind'] == 'nibli-learn-content-manifest'
assert content['registry'] == 'content/course.json'
assert content['references'] == 'content/references.json'
assert course['schema'] == course['version'] == references['schema'] == references['version'] == 4
assert content['offline_packs']['installation'] == 'verified-staging-and-explicit-activation'
assert all(content[key] == 2 for key in ['progress_schema', 'project_schema', 'operation_record_schema'])
assert content['proof_record_schema'] == 1  # Original F5 proof wrapper remains import-compatible.
assert len(content['lessons']) == len(course['lessons']) == 19
assert [item['id'] for item in content['lessons']] == [item['id'] for item in course['lessons']]
activity_ids = set()
reference_ids = {entry['id'] for entry in references['entries']}
assert len(reference_ids) == len(references['entries'])
for available, lesson in zip(content['lessons'], course['lessons'], strict=True):
    assert available['availability'] == 'available'
    assert all(available[key] == lesson[key] for key in ['id', 'version', 'track', 'unit', 'title'])
    steps = available['steps']
    assert [step['number'] for step in steps] == list(range(1, len(lesson['activities']) + 1))
    for step, activity in zip(steps, lesson['activities'], strict=True):
        assert all(step[key] == activity[key] for key in ['id', 'version', 'title', 'objective'])
        assert activity['id'] not in activity_ids
        activity_ids.add(activity['id'])
        assert set(activity['references']) <= reference_ids
    if lesson['legacy_release']:
        model = (root / 'crates/model/src/lib.rs').read_text()
        assert available['id'] == re.search(r'LESSON_ID: &str = "([^"]+)"', model)[1]
        assert len(steps) == 7
    else:
        assert available['route'] == f"#/lesson/{lesson['id']}/activity/{steps[0]['id']}"
assert len(activity_ids) == 116
assert [unit['id'] for unit in content['foundations']] == [f'F{i}' for i in range(1, 7)]
assert all(unit['availability'] == 'available' for unit in content['foundations'])
assert {item['unit'] for item in course['lessons']} == {f'F{i}' for i in range(1, 7)} | {f'U{i}' for i in range(1, 5)} | {f'I{i}' for i in range(1, 5)} | {f'B{i}' for i in range(1, 6)}
assert {track['id'] for track in content['tracks'] if track['availability'] == 'available'} == {'foundations', 'use', 'integrate', 'build'}
schema = json.loads((root / 'content/offline-pack.schema.json').read_text())
assert schema['properties']['schema']['const'] == content['offline_packs']['contract_schema']
assert content['minimum_app_schema'] in schema['properties']['app_schema']['enum']
assert schema['properties']['kind']['const'] == 'nibli-content-pack'
print(f"Content verified: 19 lessons, {len(activity_ids)} matching activities/objectives, {len(reference_ids)} references, content/app schema 4")

labs = json.loads((root / 'content/labs.json').read_text())
lab_ids = {lab['id'] for lab in labs['entries']}
assert len(lab_ids) == 11
for lesson in course['lessons']:
    for activity in lesson['activities']:
        assert activity.get('local_lab') is None or activity['local_lab'] in lab_ids
for reference in references['entries']:
    for example in reference.get('examples', []):
        assert example.get('lab') is None or example['lab'] in lab_ids

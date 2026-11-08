#!/usr/bin/env python3
"""Generate the public kit identity, or check it without changing content."""
import hashlib
import json
from pathlib import Path
import sys
root=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(root/'labs'))
from run import inventory
files=inventory();digest=hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':')).encode()).hexdigest()
rows=[
 ('compute','Compute results, failures and evidence',['memo_is_query_local','collections_do_not_dispatch','no_durable_compute_assertions','backend_failure_is_not_false']),
 ('ontology','Map an ontology without hiding losses',['fragment_roundtrip','collision_detected','main_survives_refused_detached_import','losses_exposed']),
 ('measurement','Measure distinct phases and memory quantities',['all_queries_true','distinct_shapes']),
 ('scoped-application','Keep cases and journal identities separate',['authorizer_context_isolation','case_isolation','review_support','stable_ids_after_reopen','withdrawal_survives_reopen']),
 ('wasi-runtime','Exhaust host budgets and recover committed inputs',['external_compute_is_query_local','component_identity_and_imports','fuel_trap_then_recovery','memory_trap_then_recovery','stable_journal_ids_with_tombstone','scoped_application_native_wasi_outcomes']),
]
value={'schema':1,'download':'assets/nibli-labs-v1.tar.gz','entries':[{'id':id,'version':1,'title':title,'command':f'python3 labs/run.py {id}'+(' --engine-root ../nibli-lab-engine' if id=='wasi-runtime' else '')+f' --output ../nibli-{id}-first','platform':'Verified on Linux x86_64; Rust 1.96.1 in the pinned Nix shell. Other platforms unverified.','kit_sha256':digest,'required_checks':checks} for id,title,checks in rows]}
text=json.dumps(value,indent=2)+'\n';path=root/'content/labs.json'
if '--check' in sys.argv:
 if not path.exists() or path.read_text()!=text:raise SystemExit('Local lab catalog does not match the source kit; run scripts/lab_catalog.py.')
else:path.write_text(text)
print(f'Local lab catalog: {len(rows)} labs, kit {digest}')

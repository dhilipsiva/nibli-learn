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
 ('compiler-internals','Trace the real compiler and repair its boundaries',['parser_spans_preserve_quoted_period','grammar_and_vocabulary_are_distinct','invalid_indices_refused','compile_is_not_admission','partial_parse_not_partial_commit','opaque_body_not_asserted','grouping_changes_lowering']),
 ('store-search','Build and observe a small predicate-indexed store',['predicate_index_and_clone_isolation','preloaded_mirror_refused','typed_matching_boundaries','custom_store_matches_default','real_predicate_lookups','duplicate_support_survives_one_withdrawal','withdrawal_invalidates_derivation']),
 ('rules-witnesses','Follow shared proof nodes and dependent witnesses',['multi_hop_has_real_shared_proof','proof_references_resolve_to_true','citations_present','recording_truncation_preserves_verdict','duplicate_rule_support_and_invalidation','dependent_witnesses_are_typed_and_repeatable']),
 ('runtime-buffers','Inspect ownership, interning and actual buffer copies',['same_interner_reuses_symbol','parser_tree_owns_input','split_owns_full_cloned_vectors','serialization_roundtrip','sibling_nodes_not_admitted','sessions_own_separate_state']),
 ('finite-assurance','Compare an independent finite model and repair counterexamples',['all_64_base_sets_match_finite_oracle','recording_does_not_change_decisions','order_rename_duplicate_invariance','faulty_reference_control_detected','wrong_application_rule_detected']),
 ('formal-models','Check Lean models and inspect the remaining obligations',['lean_model_files_checked','proof_cap_control_executed','coverage_inventory_bound_to_engine','no_whole_engine_certificate']),
]
value={'schema':1,'download':'assets/nibli-labs-v1.tar.gz','entries':[{'id':id,'version':1,'title':title,'command':f'python3 labs/run.py {id}'+(' --engine-root ../nibli-lab-engine' if id in {'wasi-runtime','formal-models'} else '')+f' --output ../nibli-{id}-first','platform':'Verified on Linux x86_64; Rust 1.96.1 in the pinned Nix shell. Other platforms unverified.','kit_sha256':digest,'required_checks':checks} for id,title,checks in rows]}
text=json.dumps(value,indent=2)+'\n';path=root/'content/labs.json'
if '--check' in sys.argv:
 if not path.exists() or path.read_text()!=text:raise SystemExit('Local lab catalog does not match the source kit; run scripts/lab_catalog.py.')
else:path.write_text(text)
print(f'Local lab catalog: {len(rows)} labs, kit {digest}')

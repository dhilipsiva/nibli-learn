use nibli_engine::NibliEngine;
use nibli_import::{export_facts, import_triples_raw, import_turtle};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn collisions(text: &str) -> Vec<Value> {
    let mut map = BTreeMap::<String, String>::new();
    let mut collisions = vec![];
    for triple in nibli_import::rdf::parse_turtle(text).unwrap() {
        for term in [triple.subject, triple.predicate, triple.object] {
            if let nibli_import::rdf::Term::Iri(iri) = &term {
                let name = term.local_name().to_string();
                if let Some(previous) = map.insert(name.clone(), iri.clone())
                    && &previous != iri
                {
                    collisions.push(json!({"local_name":name,"first":previous,"second":iri}));
                }
            }
        }
    }
    collisions
}
pub fn run() -> Value {
    let source = include_str!("../../fixtures/ontology-fragment.ttl");
    assert!(collisions(source).is_empty());
    let detached = NibliEngine::new();
    let imported = import_triples_raw(&detached, source).unwrap();
    let exported = export_facts(&detached);
    assert_eq!(imported, 2);
    assert_eq!(exported.exported, 2);
    assert!(exported.refused.is_empty());
    let roundtrip = NibliEngine::new();
    import_triples_raw(&roundtrip, &exported.document).unwrap();
    let again = export_facts(&roundtrip);
    assert_eq!(again.document, exported.document);
    let ambiguous = "<https://vendor-a.example/CartB> <https://workshop.example/located> <https://workshop.example/Bay7> .\n<https://vendor-b.example/CartB> <https://workshop.example/located> <https://workshop.example/Bay7> .\n";
    let conflicts = collisions(ambiguous);
    assert!(!conflicts.is_empty());
    let unsafe_candidate = NibliEngine::new();
    let imported_ambiguous = import_triples_raw(&unsafe_candidate, ambiguous).unwrap();
    let collapsed = export_facts(&unsafe_candidate);
    assert_eq!(imported_ambiguous, 2);
    assert_eq!(collapsed.exported, 1);
    // The wrapper refuses this mapping before admitting anything to a real project.
    let main = NibliEngine::new();
    main.assert_text("computer(Terminal7).").unwrap();
    let before = serde_json::to_value(main.list_assertion_records().unwrap()).unwrap();
    let partial = NibliEngine::new();
    let rejected_source = format!(
        "{source}<https://workshop.example/Eight> <https://workshop.example/exponential> <https://workshop.example/Two> .\n"
    );
    let refused = import_triples_raw(&partial, &rejected_source).unwrap_err();
    assert!(
        !partial.list_facts().unwrap().is_empty(),
        "raw import is incremental, not atomic"
    );
    // Discard the partially mutated detached candidate.
    let partial_rows = partial.list_facts().unwrap().len();
    drop(partial);
    assert_eq!(
        serde_json::to_value(main.list_assertion_records().unwrap()).unwrap(),
        before
    );
    detached
        .assert_text("past approves(Inspector, CartB).")
        .unwrap();
    let with_flavor = export_facts(&detached);
    assert!(!with_flavor.refused.is_empty());
    let sorts = NibliEngine::new();
    let sorts_source = "<https://workshop.example/Cart> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://workshop.example/Asset> .\n<https://workshop.example/CartB> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://workshop.example/Cart> .\n";
    assert_eq!(import_turtle(&sorts, sorts_source).unwrap(), 2);
    let unsupported_source =
        "[] <https://workshop.example/located> <https://workshop.example/Bay7> .";
    let bare = nibli_import::rdf::parse_turtle(unsupported_source).unwrap();
    // The engine's small reader accepts [] as a bare IRI token; it does not
    // implement Turtle blank-node semantics. This application's preflight refuses it.
    let unsupported_tokens = bare
        .iter()
        .flat_map(|t| [&t.subject, &t.predicate, &t.object])
        .filter_map(|term| {
            if let nibli_import::rdf::Term::Iri(iri) = term
                && !iri.starts_with("https://")
                && !iri.starts_with("http://")
            {
                return Some(iri.clone());
            }
            None
        })
        .collect::<Vec<_>>();
    assert_eq!(unsupported_tokens, vec!["[]"]);
    json!({"source":source,"imported":imported,"exported":exported.document,"roundtrip":again.document,"namespace_collision":{"source":ambiguous,"conflicts":conflicts,"unchecked_import_count":imported_ambiguous,"unchecked_export_count":collapsed.exported},"incremental_refusal":{"error":refused,"candidate_rows_before_discard":partial_rows,"main_unchanged":true},"flavor_refusals":with_flavor.refused,"sort_source":sorts_source,"sort_note":"Sort declarations are native configuration; the fact exporter does not export the ontology, rules, source IRIs or an application journal.","unsupported_syntax":{"source":unsupported_source,"small_reader_result":format!("{bare:?}"),"wrapper_refused_tokens":unsupported_tokens},"checks":{"fragment_roundtrip":again.document==exported.document,"collision_detected":!conflicts.is_empty(),"main_survives_refused_detached_import":true,"losses_exposed":!with_flavor.refused.is_empty()}})
}

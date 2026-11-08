//! Observe real multi-hop proofs, source support and dependent witnesses.
use nibli_session::CoreSession;
use nibli_types::execution::{RecordingOptions, RecordingStatus};
use nibli_types::logic::{ProofRule, WitnessOrigin};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn run() -> Value {
    let session = CoreSession::new();
    let source = "dog(Nori). animal(every dog). alive(every animal).";
    session.assert_text(source).unwrap();
    let recorded = session.query_text_recorded("alive(Nori).", RecordingOptions::standard());
    let (result, proof) = recorded.outcome.unwrap();
    assert!(result.is_true());
    let mut links = vec![];
    let mut incoming = vec![0_usize; proof.steps.len()];
    let mut cited = 0;
    for (i, step) in proof.steps.iter().enumerate() {
        for &child in &step.children {
            assert!((child as usize) < proof.steps.len());
            incoming[child as usize] += 1;
        }
        match &step.rule {
            ProofRule::ProofRef { .. } => {
                assert_eq!(step.children.len(), 1);
                let target = step.children[0] as usize;
                assert!(proof.steps[target].holds && step.holds);
                links.push(json!({"reference":i,"target":target}));
            }
            ProofRule::Asserted { sources, .. } => cited += sources.len(),
            ProofRule::Derived { sources, .. } => cited += sources.len(),
            _ => {}
        }
    }
    assert!(!links.is_empty());
    let shared: Vec<_> = incoming
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 1)
        .map(|(i, n)| json!({"node":i,"incoming_edges":n}))
        .collect();
    assert!(!shared.is_empty() && cited > 0);
    let mut bounded = RecordingOptions::standard();
    bounded.max_events = 4;
    let limited = session.query_text_recorded("alive(Nori).", bounded);
    assert_eq!(limited.outcome.unwrap().0, result);
    assert_eq!(limited.record.status, RecordingStatus::Truncated);
    let false_attempt = session.query_text_recorded("alive(Miso).", RecordingOptions::standard());
    assert!(false_attempt.outcome.as_ref().unwrap().0.is_false());
    let duplicate = session.assert_text("animal(every dog).").unwrap()[0].0;
    session.retract_fact(1).unwrap();
    assert!(session.query_text("alive(Nori).").unwrap().is_true());
    session.retract_fact(duplicate).unwrap();
    assert!(session.query_text("alive(Nori).").unwrap().is_false());

    let witnesses = CoreSession::new();
    witnesses
        .assert_text("person(Emi). person(Rin). every person $p: owns($p, some computer).")
        .unwrap();
    let first = witnesses.query_find_text("owns(Emi, $device).").unwrap();
    let second = witnesses.query_find_text("owns(Rin, $device).").unwrap();
    assert!(!first.is_empty() && !second.is_empty());
    let first_terms: Vec<_> = first
        .iter()
        .flatten()
        .filter(|b| b.origin == WitnessOrigin::GeneratedWitness)
        .map(|b| &b.term)
        .collect();
    let second_terms: Vec<_> = second
        .iter()
        .flatten()
        .filter(|b| b.origin == WitnessOrigin::GeneratedWitness)
        .map(|b| &b.term)
        .collect();
    assert!(!first_terms.is_empty() && !second_terms.is_empty());
    assert!(
        first_terms.iter().all(|a| !second_terms.contains(a)),
        "owners have different dependent witnesses"
    );
    assert_eq!(
        first,
        witnesses.query_find_text("owns(Emi, $device).").unwrap()
    );
    let events = recorded.record.events;
    let counts = events
        .iter()
        .fold(BTreeMap::<String, usize>::new(), |mut acc, e| {
            *acc.entry(format!("{:?}", e.kind)).or_default() += 1;
            acc
        });
    assert!(counts.get("RuleFire").copied().unwrap_or(0) > 0);
    json!({"program":source,"query":"alive(Nori).","result":result,"proof":proof,
        "proof_references":links,"shared_nodes":shared,"source_citations":cited,
        "event_counts":counts,"rule_event_sample":events.iter().filter(|e|matches!(e.kind,nibli_types::execution::ExecutionEventKind::RuleCondition | nibli_types::execution::ExecutionEventKind::RuleFire | nibli_types::execution::ExecutionEventKind::CacheHit | nibli_types::execution::ExecutionEventKind::Witness)).take(60).collect::<Vec<_>>(),
        "bounded_record":{"status":limited.record.status,"events":limited.record.events.len(),"reason":limited.record.truncation_reason},
        "failed_search":{"outcome_and_proof":false_attempt.outcome.unwrap()},
        "dependent_witnesses":{"emi":first,"rin":second},
        "remaining_source_records":session.list_assertion_records().unwrap(),
        "checks":{"multi_hop_has_real_shared_proof":true,"proof_references_resolve_to_true":true,
            "citations_present":true,"recording_truncation_preserves_verdict":true,"duplicate_rule_support_and_invalidation":true,
            "dependent_witnesses_are_typed_and_repeatable":true},
        "limits":["The bounded recorder is separate from proof construction; its truncation does not test a proof-size budget.",
            "Witness display names are not portable identities. Keep the typed term and session provenance.",
            "Public rule events expose selected state, not every private rule-template field."]})
}

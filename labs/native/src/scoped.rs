use nibli_engine::NibliEngine;
use serde_json::{Value, json};
use std::path::PathBuf;
pub fn run() -> Value {
    let directory = PathBuf::from(
        std::env::args()
            .nth(2)
            .expect("supply a fresh lab output directory"),
    );
    let db = directory.join("scoped-inspection.redb");
    assert!(!db.exists(), "do not reuse a prior experiment's journal");
    let setup = include_str!("../../fixtures/scoped-inspection.nibli");
    let engine = NibliEngine::open(&db).unwrap();
    for line in setup.lines().filter(|l| !l.trim().is_empty()) {
        engine.assert_text(line).unwrap();
    }
    let claim = "ready(CartB, CaseA).";
    let wrong_case = engine.query_holds(claim).unwrap();
    assert!(wrong_case.is_false());
    let added = engine.assert_text("approves(Reviewer, CaseA).").unwrap();
    assert_eq!(added.len(), 1);
    let (correct, proof_text, proof_json) = engine.query_text_with_proof(claim).unwrap();
    assert!(correct.is_true());
    let protected = engine.assert_text(claim).unwrap_err().to_string();
    let review_id = added[0];
    let before = engine.list_assertion_records().unwrap();
    drop(engine);
    let reopened = NibliEngine::open(&db).unwrap();
    assert!(reopened.query_holds(claim).unwrap().is_true());
    let after = reopened.list_assertion_records().unwrap();
    assert_eq!(
        serde_json::to_value(&before).unwrap(),
        serde_json::to_value(&after).unwrap()
    );
    reopened.retract_fact(review_id).unwrap();
    assert!(reopened.query_holds(claim).unwrap().is_false());
    drop(reopened);
    let reopened_again = NibliEngine::open(&db).unwrap();
    let after_withdrawal = reopened_again.query_holds(claim).unwrap();
    assert!(after_withdrawal.is_false());
    let mut authorizer = nibli_auth::Authorizer::new();
    authorizer
        .load_policy(Some(
            "all $agent, $case: approves($agent, $case) -> authorized($agent, \"inspect\", $case).",
        ))
        .unwrap();
    let scoped_allow = authorizer
        .can(
            "Inspector",
            "inspect",
            "CaseA",
            "approves(Inspector, CaseA).",
        )
        .unwrap();
    let wrong_resource = authorizer
        .can(
            "Inspector",
            "inspect",
            "CaseB",
            "approves(Inspector, CaseA).",
        )
        .unwrap();
    let context_cleared = authorizer.can("Inspector", "inspect", "CaseA", "").unwrap();
    assert!(scoped_allow.allowed);
    assert!(!wrong_resource.allowed);
    assert!(!context_cleared.allowed);
    let decision_adapter = [
        nibli_types::logic::QueryResult::True,
        nibli_types::logic::QueryResult::False,
        nibli_types::logic::QueryResult::Unknown(
            nibli_types::logic::UnknownReason::BackendUnavailable,
        ),
        nibli_types::logic::QueryResult::ResourceExceeded(nibli_types::logic::ResourceKind::Fuel),
    ]
    .iter()
    .map(|outcome| {
        let decision = nibli_auth::Decision::from_query(outcome, None);
        assert_eq!(decision.allowed, outcome.is_true());
        json!({"supplied_adapter_input":outcome,"allowed":decision.allowed})
    })
    .collect::<Vec<_>>();
    json!({"authorizer":{"scoped_allow":format!("{scoped_allow:?}"),"wrong_resource":format!("{wrong_resource:?}"),"context_cleared":format!("{context_cleared:?}"),"decision_adapter_controls":decision_adapter},"setup":setup,"claim":claim,"wrong_case":wrong_case,"review_id":review_id.to_string(),"correct":correct,"proof":serde_json::from_str::<Value>(&proof_json).unwrap(),"proof_text":proof_text,"protected_assertion_refusal":protected,"journal_before":before,"journal_reopened":after,"after_withdrawal_and_reopen":after_withdrawal,"checks":{"authorizer_context_isolation":scoped_allow.allowed&&!wrong_resource.allowed&&!context_cleared.allowed,"case_isolation":wrong_case.is_false(),"review_support":correct.is_true(),"stable_ids_after_reopen":true,"withdrawal_survives_reopen":after_withdrawal.is_false()}})
}

//! Independent finite least-fixed-point oracle for a deliberately small fragment.
use nibli_session::CoreSession;
use nibli_types::execution::RecordingOptions;
use serde_json::{Value, json};

const NAMES: [&str; 3] = ["Nori", "Miso", "Tavi"];
const RELATIONS: [&str; 3] = ["dog", "animal", "alive"];
// This oracle neither calls Nibli nor shares its parser, terms, unifier or search.
fn closure(seed: [[bool; 3]; 3]) -> [[bool; 3]; 3] {
    let mut rows = seed;
    loop {
        let before = rows;
        for (entity, _) in NAMES.iter().enumerate() {
            rows[1][entity] |= rows[0][entity];
            rows[2][entity] |= rows[1][entity];
        }
        if rows == before {
            return rows;
        }
    }
}
fn session(sources: &[String]) -> CoreSession {
    let core = CoreSession::new();
    for source in sources {
        core.assert_text(source).unwrap();
    }
    core
}
pub fn run() -> Value {
    let mut cases = vec![];
    let mut query_checks = 0;
    let mut metamorphic_checks = 0;
    for mask in 0..64 {
        let mut seed = [[false; 3]; 3];
        let mut sources = vec![
            "all $x: dog($x) -> animal($x).".into(),
            "all $x: animal($x) -> alive($x).".into(),
        ];
        for relation in 0..2 {
            for (entity, name) in NAMES.iter().enumerate() {
                if mask & (1 << (relation * 3 + entity)) != 0 {
                    seed[relation][entity] = true;
                    sources.push(format!("{}({name}).", RELATIONS[relation]));
                }
            }
        }
        let expected = closure(seed);
        let core = session(&sources);
        let mut answers = vec![];
        for (relation, predicate) in RELATIONS.iter().enumerate() {
            for (entity, name) in NAMES.iter().enumerate() {
                let query = format!("{predicate}({name}).");
                let plain = core.query_text(&query).unwrap();
                let recorded = core.query_text_recorded(&query, RecordingOptions::standard());
                assert_eq!(plain, recorded.outcome.unwrap().0);
                assert!(
                    plain.is_true() || plain.is_false(),
                    "inconclusive does not count as an oracle match"
                );
                assert_eq!(
                    plain.is_true(),
                    expected[relation][entity],
                    "mask {mask}, {query}"
                );
                query_checks += 1;
                answers.push(
                    json!({"query":query,"oracle":expected[relation][entity],"observed":plain}),
                );
            }
        }
        let baseline = core.query_text("alive(Nori).").unwrap();
        let mut reversed = sources.clone();
        reversed.reverse();
        assert_eq!(
            session(&reversed).query_text("alive(Nori).").unwrap(),
            baseline
        );
        let renamed: Vec<_> = sources.iter().map(|s| s.replace("Nori", "Emi")).collect();
        assert_eq!(
            session(&renamed).query_text("alive(Emi).").unwrap(),
            baseline
        );
        let mut duplicates = sources.clone();
        duplicates.extend(sources.clone());
        assert_eq!(
            session(&duplicates).query_text("alive(Nori).").unwrap(),
            baseline
        );
        metamorphic_checks += 3;
        cases.push(json!({"seed_mask":mask,"answers":answers}));
    }
    // A locally authored wrong closure that omits the animal→alive edge is killed.
    let mut counterexample = [[false; 3]; 3];
    counterexample[0][0] = true;
    assert!(closure(counterexample)[2][0]);
    let faulty_reference_alive = counterexample[2][0];
    assert!(!faulty_reference_alive);
    // A separate bad application encoding (OR instead of AND) is actually run.
    let shared = ["approves(TestRunner, CartC).".into()];
    let good = session(&shared);
    let bad = session(&shared);
    good.assert_text("all $c: approves(TestRunner, $c) & approves(Reviewer, $c) -> ready($c).")
        .unwrap();
    bad.assert_text("all $c: approves(TestRunner, $c) | approves(Reviewer, $c) -> ready($c).")
        .unwrap();
    let correct = good.query_text("ready(CartC).").unwrap();
    let mutant = bad.query_text("ready(CartC).").unwrap();
    assert!(correct.is_false() && mutant.is_true());
    json!({"oracle_domain":{"named_entities":NAMES,"unary_predicates":RELATIONS,"rules":["dog(x) ⇒ animal(x)","animal(x) ⇒ alive(x)"],"varying_base_facts":6,"all_subsets":64},
        "finite_cases":cases,"query_checks":query_checks,"recorded_plain_comparisons":query_checks,"metamorphic_checks":metamorphic_checks,
        "counterexamples":{"missing_oracle_edge":{"expected_alive":true,"faulty_reference_alive":faulty_reference_alive},"application_or_mutation":{"correct_and":correct,"wrong_or":mutant}},
        "checks":{"all_64_base_sets_match_finite_oracle":true,"recording_does_not_change_decisions":true,
            "order_rename_duplicate_invariance":true,"faulty_reference_control_detected":true,"wrong_application_rule_detected":true},
        "limits":["Exhaustive only over these 64 positive unary base sets with these two fixed acyclic rules and three names.",
            "No oracle coverage for negation, equality, existential heads, arithmetic, external compute, budgets or arbitrary programs.",
            "The faulty reference and application-rule controls are original examples, not a mutation campaign against engine source.",
            "A differential agreement can share an unnoticed specification mistake. Review the oracle's domain and invariants."]})
}

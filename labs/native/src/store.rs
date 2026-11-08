//! A small real FactStore: one predicate index, plus aggregate observation counters.
use nibli_reason::KnowledgeBase;
use nibli_reason::fact_store::FactStore;
use nibli_reason::kb::{
    GroundFact, GroundTerm, StoredFact, substitute_fact, unify_facts,
    validate_stored_fact_groundness,
};
use nibli_session::CoreSession;
use nibli_types::execution::RecordingOptions;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

type Counters = Arc<Mutex<BTreeMap<String, usize>>>;
#[derive(Clone, Default)]
struct LearningStore {
    buckets: HashMap<String, HashSet<StoredFact>>,
    observations: Counters,
}
impl LearningStore {
    fn note(&self, key: String) {
        *self.observations.lock().unwrap().entry(key).or_default() += 1;
    }
}
impl FactStore for LearningStore {
    fn lookup_predicate(&self, relation: &str) -> Option<&HashSet<StoredFact>> {
        self.note(format!("lookup:{relation}"));
        self.buckets.get(relation)
    }
    fn contains(&self, fact: &StoredFact) -> bool {
        self.note("contains".into());
        self.buckets
            .get(fact.relation())
            .is_some_and(|rows| rows.contains(fact))
    }
    fn insert(&mut self, fact: StoredFact) {
        self.note("insert".into());
        self.buckets
            .entry(fact.relation().into())
            .or_default()
            .insert(fact);
    }
    fn clear(&mut self) {
        self.note("clear".into());
        self.buckets.clear();
    }
    fn all_facts(&self) -> Box<dyn Iterator<Item = &StoredFact> + '_> {
        self.note("all_facts".into());
        Box::new(self.buckets.values().flatten())
    }
    fn len(&self) -> usize {
        self.buckets.values().map(HashSet::len).sum()
    }
    fn remove(&mut self, fact: &StoredFact) -> bool {
        self.note("remove".into());
        let Some(rows) = self.buckets.get_mut(fact.relation()) else {
            return false;
        };
        let removed = rows.remove(fact);
        if rows.is_empty() {
            self.buckets.remove(fact.relation());
        }
        removed
    }
    fn clone_box(&self) -> Box<dyn FactStore> {
        self.note("clone_box".into());
        Box::new(self.clone())
    }
}
fn fact(relation: &str, args: Vec<GroundTerm>) -> StoredFact {
    StoredFact::Bare(GroundFact::new(relation, args))
}
fn constant(name: &str) -> GroundTerm {
    GroundTerm::Constant(name.into())
}
fn variable(name: &str) -> GroundTerm {
    GroundTerm::PatternVar(name.into())
}

pub fn run() -> Value {
    let one = fact("owns", vec![constant("Emi"), constant("Console4")]);
    let mut primitive = LearningStore::default();
    primitive.insert(one.clone());
    primitive.insert(one.clone());
    assert_eq!(primitive.len(), 1);
    assert!(primitive.lookup_predicate("dog").is_none());
    assert_eq!(primitive.lookup_predicate("owns").unwrap().len(), 1);
    let mut cloned = primitive.clone_box();
    assert!(cloned.remove(&one));
    assert!(cloned.is_empty() && primitive.contains(&one));
    let preload_error = match KnowledgeBase::with_store(Box::new(primitive)) {
        Err(error) => error,
        Ok(_) => panic!("preloaded mirror has no assertion registry"),
    };

    let repeated = fact("owns", vec![variable("x"), variable("x")]);
    let same = fact("owns", vec![constant("Emi"), constant("Emi")]);
    let bindings = unify_facts(&repeated, &same).unwrap();
    assert_eq!(substitute_fact(&repeated, &bindings), same);
    assert!(unify_facts(&repeated, &one).is_none());
    let wrong_arity = fact("owns", vec![constant("Emi")]);
    let wrapped = StoredFact::Past(same.inner().clone());
    assert!(unify_facts(&repeated, &wrong_arity).is_none());
    assert!(unify_facts(&repeated, &wrapped).is_none());
    let unspecified = fact("owns", vec![constant("Emi"), GroundTerm::Unspecified]);
    assert!(unify_facts(&unspecified, &one).is_none());
    let groundness_error = validate_stored_fact_groundness(&repeated).unwrap_err();
    validate_stored_fact_groundness(&one).unwrap();

    let store = LearningStore::default();
    let counters = store.observations.clone();
    let custom = CoreSession::with_kb(KnowledgeBase::with_store(Box::new(store)).unwrap());
    let default = CoreSession::new();
    let program = "dog(Nori). all $x: dog($x) -> animal($x). all $x: animal($x) -> alive($x).";
    custom.assert_text(program).unwrap();
    default.assert_text(program).unwrap();
    for i in 0..32 {
        let irrelevant = format!("computer(Panel{i}).");
        custom.assert_text(&irrelevant).unwrap();
        default.assert_text(&irrelevant).unwrap();
    }
    counters.lock().unwrap().clear();
    let recorded = custom.query_text_recorded("alive(Nori).", RecordingOptions::standard());
    let (answer, proof) = recorded.outcome.unwrap();
    let default_answer = default.query_text("alive(Nori).").unwrap();
    assert_eq!(answer, default_answer);
    assert!(answer.is_true());
    let search_counters = counters.lock().unwrap().clone();
    assert!(search_counters.keys().any(|key| key.starts_with("lookup:")));
    let wrong = custom.query_text("alive(Miso).").unwrap();
    assert!(wrong.is_false());
    let materialization_before = custom.materialization_report().unwrap();
    custom.set_materialization(true);
    assert_eq!(custom.query_text("alive(Nori).").unwrap(), answer);
    let materialization_on = custom.materialization_report().unwrap();

    let duplicate = custom.assert_text("dog(Nori).").unwrap()[0].0;
    custom.retract_fact(0).unwrap();
    let one_support = custom.query_text("alive(Nori).").unwrap();
    assert!(one_support.is_true());
    custom.retract_fact(duplicate).unwrap();
    let cleared_report = custom.materialization_report().unwrap();
    assert!(cleared_report.0.is_empty() && cleared_report.1.is_empty());
    let no_support = custom.query_text("alive(Nori).").unwrap();
    assert!(
        no_support.is_false(),
        "withdrawal must invalidate cached derivations"
    );
    let observations = counters.lock().unwrap().clone();
    assert!(observations.get("clear").copied().unwrap_or(0) > 0);
    let event_counts =
        recorded
            .record
            .events
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut acc, event| {
                *acc.entry(format!("{:?}", event.kind)).or_default() += 1;
                acc
            });
    json!({"preloaded_mirror_refusal":preload_error,"typed_matching":{"template":repeated,"bindings":bindings,
        "substitution":same,"groundness_refusal":groundness_error},
        "program":program,"query":"alive(Nori).","irrelevant_sources":32,
        "custom_answer":answer,"default_answer":default_answer,"wrong_name":wrong,
        "query_store_calls":search_counters,"all_store_calls_after_repairs":observations,
        "recorded_event_counts":event_counts,"recorded_search_sample":recorded.record.events.iter().filter(|e| e.phase != nibli_types::execution::ExecutionPhase::Compile).take(80).collect::<Vec<_>>(),"proof":proof,
        "materialization":{"before":materialization_before,"enabled":materialization_on,"after_withdraw_before_query":cleared_report},
        "support_repair":{"duplicate_id":duplicate,"after_first_withdrawal":one_support,"after_both":no_support},
        "checks":{"predicate_index_and_clone_isolation":true,"preloaded_mirror_refused":true,"typed_matching_boundaries":true,
            "custom_store_matches_default":true,"real_predicate_lookups":true,"duplicate_support_survives_one_withdrawal":true,
            "withdrawal_invalidates_derivation":true},
        "limits":["This small store is memory-only, with no durability, concurrency or crash-recovery claim.",
            "Clones copy fact buckets but share observation counters, so counters aggregate candidate/rebuild work.",
            "Lookup counts describe calls at this interface, not total engine work or a timing benchmark.",
            "The engine validates groundness and owns source records; the infallible storage trait is a trusted mirror."]})
}

//! Actual native dispatch and TCP-JSONL controls. No model or remote endpoint.
use nibli_engine::NibliEngine;
use nibli_session::CoreSession;
use nibli_types::logic::{LogicBuffer, LogicNode, LogicalTerm, QueryResult, UnknownReason};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
static CALLS: AtomicUsize = AtomicUsize::new(0);
fn evaluate(relation: &str, args: &[LogicalTerm]) -> Result<bool, String> {
    CALLS.fetch_add(1, Ordering::SeqCst);
    if relation != "exponential" {
        return Err("unsupported relation".into());
    }
    match args {
        [
            LogicalTerm::Number(result),
            LogicalTerm::Number(base),
            LogicalTerm::Number(power),
        ] if result.is_finite() && base.is_finite() && power.is_finite() => {
            let actual = base.powf(*power);
            if actual.is_finite() {
                Ok(actual == *result)
            } else {
                Err("non-finite backend result".into())
            }
        }
        _ => Err("backend requires exactly three finite numbers".into()),
    }
}
fn batch(requests: &[nibli_reason::ComputeRequest]) -> Vec<Result<bool, String>> {
    requests
        .iter()
        .map(|r| evaluate(&r.relation, &r.args))
        .collect()
}
fn tcp(reply: &'static str, delay: Duration) -> Value {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let thread = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        let parsed: Value = serde_json::from_str(&request).unwrap();
        assert_eq!(parsed["relation"], "exponential");
        thread::sleep(delay);
        let _ = writeln!(stream, "{reply}");
        parsed
    });
    let mut engine = NibliEngine::new();
    engine.enable_compute_backend(&address.to_string());
    engine
        .register_compute_predicate("exponential".into())
        .unwrap();
    let began = Instant::now();
    let result = engine.query_holds("exponential(8, 2, 3).").unwrap();
    let elapsed_ms = began.elapsed().as_millis();
    let request = thread.join().unwrap();
    json!({"request":request,"reply":reply,"result":result,"elapsed_ms":elapsed_ms,"assertions_after":engine.list_assertion_records().unwrap().len()})
}
pub fn run() -> Value {
    let mut core = CoreSession::new();
    let unknown_registration = core
        .register_compute_predicate("invented_inspection_service".into())
        .unwrap_err()
        .to_string();
    core.register_compute_predicate("exponential".into())
        .unwrap();
    let no_backend = core.query_text("exponential(8, 2, 3).").unwrap();
    assert_eq!(
        no_backend,
        QueryResult::Unknown(UnknownReason::BackendUnavailable)
    );
    core.set_compute_dispatch(evaluate, batch);
    CALLS.store(0, Ordering::SeqCst);
    let (memo_result, proof) = core
        .query_text_with_proof("exponential(8, 2, 3) & exponential(8, 2, 3).")
        .unwrap();
    let first_calls = CALLS.load(Ordering::SeqCst);
    assert!(memo_result.is_true());
    assert_eq!(
        first_calls, 1,
        "identical compute checks share query-local evidence"
    );
    assert!(core.query_text("exponential(8, 2, 3).").unwrap().is_true());
    let second_calls = CALLS.load(Ordering::SeqCst);
    assert_eq!(second_calls, 2, "new top-level query must redispatch");
    assert!(core.query_text("exponential(9, 2, 3).").unwrap().is_false());
    let before_find = CALLS.load(Ordering::SeqCst);
    let find = core
        .query_find_text_outcome("exponential($value, 2, 3).")
        .unwrap();
    let count = core
        .count_witnesses_text_outcome("exponential($value, 2, 3).")
        .unwrap();
    let aggregate = core
        .aggregate_text_outcome(
            "exponential($value, 2, 3).",
            "$value",
            nibli_types::logic::AggregateOp::Sum,
        )
        .unwrap();
    let after_find = CALLS.load(Ordering::SeqCst);
    assert_eq!(after_find, before_find, "collections must not dispatch");
    let rejected = core
        .assert_text("exponential(8, 2, 3).")
        .unwrap_err()
        .to_string();
    assert!(core.list_assertion_records().unwrap().is_empty());
    let snapshot = core.snapshot().unwrap();
    let restored = CoreSession::from_snapshot(&snapshot).unwrap();
    let after_restore = restored.query_text("exponential(8, 2, 3).").unwrap();
    assert_eq!(
        after_restore, no_backend,
        "dispatch callbacks are not serialized"
    );
    restored.set_compute_dispatch(evaluate, batch);
    assert!(
        restored
            .query_text("exponential(8, 2, 3).")
            .unwrap()
            .is_true()
    );
    let divide_zero = core.query_text("quotient(1, 2, 0).").unwrap();
    assert!(divide_zero.is_false());
    let raw_non_finite = core
        .kb()
        .query_entailment(LogicBuffer {
            nodes: vec![LogicNode::ComputeNode((
                "product".into(),
                vec![
                    LogicalTerm::Number(1.0),
                    LogicalTerm::Number(f64::INFINITY),
                    LogicalTerm::Number(2.0),
                ],
            ))],
            roots: vec![0],
        })
        .unwrap();
    assert_eq!(
        raw_non_finite,
        QueryResult::Unknown(UnknownReason::BackendUnavailable)
    );
    // The source compiler emits event-group structure. Preserve that structure
    // when injecting a non-finite test operand; flat raw IR has a different host contract.
    let mut numerical = core.compile_query_text("product(1, 2, 3).").unwrap();
    for node in &mut numerical.nodes {
        match node {
            LogicNode::ComputeNode((_, args)) | LogicNode::Predicate((_, args)) => {
                for term in args {
                    if matches!(term, LogicalTerm::Number(n) if *n == 2.0) {
                        *term = LogicalTerm::Number(f64::INFINITY);
                    }
                }
            }
            _ => {}
        }
    }
    let non_finite = core.kb().query_entailment(numerical).unwrap();
    assert_eq!(non_finite, QueryResult::Unknown(UnknownReason::NonFinite));
    let argument_refusal = core
        .query_text("exponential(UnknownNumber, 2, 3).")
        .unwrap();
    assert_eq!(
        argument_refusal, no_backend,
        "backend argument errors stay unavailable"
    );
    let mut conflict = CoreSession::new();
    conflict.assert_text("person(Operator).").unwrap();
    let live_conflict = conflict
        .register_compute_predicate("person".into())
        .unwrap_err()
        .to_string();
    let tcp_true = tcp(r#"{"result":true}"#, Duration::ZERO);
    let tcp_false = tcp(r#"{"result":false}"#, Duration::ZERO);
    let tcp_malformed = tcp("not-json", Duration::ZERO);
    // The runner sets a one-second read timeout, isolated to this child process.
    assert_eq!(
        std::env::var("NIBLI_BACKEND_READ_TIMEOUT_SECS").as_deref(),
        Ok("1")
    );
    let tcp_timeout = tcp(r#"{"result":true}"#, Duration::from_millis(1800));
    assert_eq!(tcp_true["result"], json!(QueryResult::True));
    assert!(
        serde_json::from_value::<QueryResult>(tcp_false["result"].clone())
            .unwrap()
            .is_false()
    );
    for result in [&tcp_malformed, &tcp_timeout] {
        assert_eq!(result["result"], json!(no_backend));
    }
    json!({"unknown_registration":unknown_registration,"live_registration_conflict":live_conflict,"unavailable":no_backend,"memo_query":memo_result,"first_query_dispatches":first_calls,"two_queries_dispatches":second_calls,"proof":proof,"find":find,"count":count,"aggregate":aggregate,"collection_dispatches":0,"assertion_refusal":rejected,"durable_assertions":0,"restore_requires_host_rebind":after_restore,"divide_by_zero":divide_zero,"non_finite":non_finite,"raw_flat_non_finite_host_error":raw_non_finite,"argument_refusal":argument_refusal,"tcp":{"true":tcp_true,"false":tcp_false,"malformed":tcp_malformed,"timeout":tcp_timeout},"checks":{"memo_is_query_local":first_calls==1&&second_calls==2,"collections_do_not_dispatch":after_find==before_find,"no_durable_compute_assertions":core.list_assertion_records().unwrap().is_empty(),"backend_failure_is_not_false":true}})
}

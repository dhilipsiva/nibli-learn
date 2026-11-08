//! Follow actual parser/emitter/lowering boundaries with original inputs.
use nibli_session::CoreSession;
use nibli_types::execution::RecordingOptions;
use nibli_types::logic::{LogicBuffer, LogicNode};
use serde_json::{Value, json};

pub fn run() -> Value {
    let source = "knows(Emi, \"inspection.v2\").\ncomputer(Console4).";
    let parsed = nibli_kr::parser::parse_statements(source).unwrap();
    let spans: Vec<_> = parsed
        .iter()
        .map(|s| json!({"start":s.span.start,"end":s.span.end,"source":&source[s.span.clone()]}))
        .collect();
    assert_eq!(parsed.len(), 2, "the quoted period is not a statement end");
    let ast = nibli_kr::parse_checked(source).unwrap();
    let flat = json!({
        "predicates":ast.predicates.iter().map(|v|format!("{v:?}")).collect::<Vec<_>>(),
        "arguments":ast.arguments.iter().map(|v|format!("{v:?}")).collect::<Vec<_>>(),
        "sentences":ast.sentences.iter().map(|v|format!("{v:?}")).collect::<Vec<_>>(),
        "roots":ast.roots,
    });
    let lowered = nibli_semantics::compile_from_ast(ast.clone()).unwrap();
    let mut invalid_ast = ast;
    invalid_ast.roots = vec![u32::MAX];
    let corrupt_ast = nibli_semantics::compile_from_ast(invalid_ast)
        .unwrap_err()
        .to_string();

    let missing_period = "computer(Console4)";
    let grammar_error = nibli_kr::parser::parse_statements(missing_period)
        .unwrap_err()
        .to_string();
    let unknown = "unregistered_inspection_word(Console4).";
    assert!(nibli_kr::parser::parse_statements(unknown).is_ok());
    let vocabulary_error = nibli_kr::parse_checked(unknown).unwrap_err().to_string();

    let session = CoreSession::new();
    let recorded = session.compile_text_recorded(source, true, RecordingOptions::standard());
    assert!(recorded.outcome.is_ok());
    assert!(session.list_assertion_records().unwrap().is_empty());
    let recorded_spans = &recorded.record.artifacts[0].statement_spans;
    assert_eq!(recorded_spans.len(), parsed.len());
    for (recorded_span, statement) in recorded_spans.iter().zip(&parsed) {
        assert_eq!(
            (recorded_span.start, recorded_span.end),
            (statement.span.start, statement.span.end)
        );
    }
    let query_only =
        session.compile_text_recorded("sum(7, 3, 4).", true, RecordingOptions::standard());
    assert!(query_only.outcome.is_ok(), "query arithmetic compiles");
    let admission_error = session
        .assert_text("sum(7, 3, 4).")
        .unwrap_err()
        .to_string();
    let mixed = "computer(Console4). unregistered_inspection_word(Console4).";
    let recovery = nibli_kr::parse_text(mixed);
    assert_eq!(recovery.buffer.roots.len(), 1);
    assert_eq!(recovery.errors.len(), 1);
    let atomic_error = session.assert_text(mixed).unwrap_err().to_string();
    assert!(
        session.list_assertion_records().unwrap().is_empty(),
        "partial parser recovery is not admission"
    );
    session
        .assert_text("knows(Emi, fact { broken(Console4) }).")
        .unwrap();
    let outer = session
        .query_text("knows(Emi, fact { broken(Console4) }).")
        .unwrap();
    let inner = session.query_text("broken(Console4).").unwrap();
    assert!(outer.is_true() && inner.is_false());

    let examples = [
        ("precedence", "dog(Nori) | animal(Nori) & alive(Nori)."),
        (
            "explicit-grouping",
            "(dog(Nori) | animal(Nori)) & alive(Nori).",
        ),
        ("exclusive-alternative", "dog(Nori) ^ animal(Nori)."),
        ("two-directions", "dog(Nori) <-> animal(Nori)."),
        (
            "dependent-owner",
            "every computer $c: owns(some person, $c).",
        ),
        (
            "relative-binding",
            "owns(Emi, some computer where broken(it)).",
        ),
        ("property-slot", "knows(Emi, property { broken(slot) })."),
    ]
    .into_iter()
    .map(|(name, text)| {
        let result = session.compile_text(text).unwrap();
        json!({"name":name,"source":text,"logic":result})
    })
    .collect::<Vec<_>>();
    assert_ne!(examples[0]["logic"], examples[1]["logic"]);
    let scope_refusal = session
        .compile_text("owns(Emi, some computer where broken(Console4)).")
        .unwrap_err()
        .to_string();
    let raw = LogicBuffer {
        nodes: vec![LogicNode::NotNode(9)],
        roots: vec![0],
    };
    let raw_error = session.kb().query_entailment(raw).unwrap_err().to_string();
    json!({
        "source":source,"parser_spans":spans,"owned_parser_tree_debug":format!("{parsed:#?}"),
        "flat_ast_debug":flat,"lowered_logic":lowered,"compiler_record":recorded.record,
        "failures":{"grammar":grammar_error,"vocabulary":vocabulary_error,"corrupt_flat_ast":corrupt_ast,
            "query_only_admission":admission_error,"mixed_atomic_admission":atomic_error,
            "relative_without_binding":scope_refusal,"corrupt_raw_logic":raw_error},
        "query_only_inspection":query_only.record,"partial_recovery":{"roots":recovery.buffer.roots.len(),"errors":recovery.errors.len()},
        "opaque_content":{"outer":outer,"inner":inner},"lowering_examples":examples,
        "checks":{"parser_spans_preserve_quoted_period":true,"grammar_and_vocabulary_are_distinct":true,
            "invalid_indices_refused":true,"compile_is_not_admission":true,"partial_parse_not_partial_commit":true,
            "opaque_body_not_asserted":true,"grouping_changes_lowering":true},
        "limits":["Parser-tree and flat-AST strings are debug inspection, not stable wire formats.",
            "Examples exercise the pinned compiler; they are not a proof of every grammar production.",
            "A structurally valid assertion can still be refused by a particular knowledge base."]
    })
}

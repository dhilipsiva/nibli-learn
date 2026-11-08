//! Observe ownership at actual public compiler/buffer/session boundaries.
use nibli_semantics::semantic::SemanticCompiler;
use nibli_session::CoreSession;
use nibli_types::logic::LogicBuffer;
use serde_json::{Value, json};

pub fn run() -> Value {
    let mut compiler = SemanticCompiler::new();
    let first = compiler.interner.get_or_intern("Console4");
    let repeated = compiler.interner.get_or_intern("Console4");
    let other = compiler.interner.get_or_intern("Console5");
    assert_eq!(first, repeated);
    assert_ne!(first, other);
    let resolved = compiler.interner.resolve(&first).to_owned();
    assert_eq!(resolved, "Console4");
    let source = String::from("computer(Console4). broken(Console5).");
    let parsed = nibli_kr::parser::parse_statements(&source).unwrap();
    let ast = nibli_kr::parse_checked(&source).unwrap();
    drop(source);
    // The public parser AST owns its names. It remains usable after input drop.
    let owned_tree = format!("{parsed:?}");
    assert!(owned_tree.contains("Console4"));
    let (logic, compile_cost) = crate::measurement::phase("flatten-owned-ast", || {
        nibli_semantics::compile_from_ast(ast).unwrap()
    });
    assert_eq!(logic.roots.len(), 2);
    let (split, split_cost) = crate::measurement::phase("split-roots", || logic.split_roots());
    assert_eq!(split.len(), 2);
    for part in &split {
        assert_eq!(part.nodes, logic.nodes);
        assert_ne!(
            part.nodes.as_ptr(),
            logic.nodes.as_ptr(),
            "each split owns a cloned node Vec"
        );
    }
    assert_ne!(split[0].nodes.as_ptr(), split[1].nodes.as_ptr());
    let node_count = logic.nodes.len();
    let (wire, serialize_cost) =
        crate::measurement::phase("json-encode-logic", || serde_json::to_vec(&logic).unwrap());
    let (roundtrip, decode_cost) = crate::measurement::phase("json-decode-logic", || {
        serde_json::from_slice::<LogicBuffer>(&wire).unwrap()
    });
    assert_eq!(roundtrip, logic);
    drop(logic);
    let core = CoreSession::new();
    core.kb()
        .assert_fact(split[0].clone(), "first compiled root".into())
        .unwrap();
    assert!(core.query_text("computer(Console4).").unwrap().is_true());
    assert!(
        core.query_text("broken(Console5).").unwrap().is_false(),
        "unreachable sibling nodes are inert"
    );
    let separate = CoreSession::new();
    assert!(
        separate
            .query_text("computer(Console4).")
            .unwrap()
            .is_false()
    );
    drop(core);
    assert!(separate.list_assertion_records().unwrap().is_empty());
    json!({"interning":{"first_debug":format!("{first:?}"),"repeated_debug":format!("{repeated:?}"),"other_debug":format!("{other:?}"),"resolved":resolved},
        "owned_parser_tree":owned_tree,"flat_nodes":node_count,"split_roots":split.iter().map(|b|json!({"nodes":b.nodes.len(),"roots":b.roots})).collect::<Vec<_>>(),
        "json_bytes":wire.len(),"measurements":[compile_cost,split_cost,serialize_cost,decode_cost],
        "checks":{"same_interner_reuses_symbol":true,"parser_tree_owns_input":true,"split_owns_full_cloned_vectors":true,
            "serialization_roundtrip":true,"sibling_nodes_not_admitted":true,"sessions_own_separate_state":true},
        "limits":["Interned keys are local to their compiler. The flattened LogicBuffer resolves them to owned strings.",
            "JSON bytes and allocation counts describe this native experiment, not canonical-ABI transfer costs.",
            "split_roots preserves indices by cloning every node vector; shared indices are not shared allocations.",
            "This lab uses native sessions. Use the WASI lab for the actual component's typed interface and guest imports."]})
}

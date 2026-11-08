//! Every published syntax example is checked by the real worker compiler.
use nibli_learn_model::*;
use nibli_learn_worker::Runtime;
use serde_json::json;

#[test]
fn reference_examples_compile_kr_and_label_external_execution_contexts() {
    let course = Course::parse(include_str!("../../../content/course.json")).unwrap();
    let library =
        ReferenceLibrary::parse(include_str!("../../../content/references.json"), &course).unwrap();
    let mut runtime = Runtime::default();
    let initialized = runtime
        .handle(Request {
            id: "reference-initialize".into(),
            generation: "1".into(),
            expected_epoch: None,
            expected_revision: None,
            command: Command::InitializeProfile {
                epoch: "reference-fixture".into(),
                statements: vec![],
                profile: RunProfile::default(),
                recording: RecordingLevel::Standard,
            },
        })
        .result
        .unwrap();
    let baseline = serde_json::to_value(&initialized.session).unwrap();
    let mut failures = vec![];
    let mut observations = vec![];
    for entry in &library.entries {
        for (index, example) in entry.examples.iter().enumerate() {
            let label = format!("{}/{index}", entry.id);
            if example.kind != ExampleKind::Kr {
                assert!(example.expected.is_none());
                assert!(example.run_in.as_ref().is_some_and(|s| !s.is_empty()));
                observations.push(json!({"reference":entry.id,"example":index,"kind":example.kind,"run_in":example.run_in,"lab":example.lab,"verification":"execution belongs to the named local lab or browser contract; not a KR compilation"}));
                continue;
            }
            let Some(expected) = &example.expected else {
                failures.push(format!(
                    "{label}: authored example has no compile expectation"
                ));
                continue;
            };
            if expected.family == "failed"
                && (expected.stage.as_deref() != Some("compile") || expected.error_class.is_none())
            {
                failures.push(format!(
                    "{label}: refused example lacks its actual stage/class"
                ));
            }
            let reply = runtime
                .handle(Request {
                    id: label.clone(),
                    generation: "1".into(),
                    expected_epoch: Some(initialized.session.epoch.clone()),
                    expected_revision: Some(initialized.session.revision.clone()),
                    command: Command::Execute {
                        operation: Operation::Compile {
                            source: example.source.clone(),
                            structural_preflight: false,
                        },
                        activity_id: None,
                        content_version: course.version,
                    },
                })
                .result;
            let actual = match reply {
                Ok(reply) => {
                    assert_eq!(
                        serde_json::to_value(&reply.session).unwrap(),
                        baseline,
                        "compilation must not admit or change state: {label}"
                    );
                    reply
                        .operation
                        .expect("compile returns an operation")
                        .result
                }
                Err(error) => error.as_operation_failure("compile"),
            };
            let matched = expected.matches(&actual);
            observations.push(
                json!({"reference":entry.id,"example":index,"source":example.source,
                "expected":expected,"actual":actual,"matched":matched}),
            );
            if !matched {
                failures.push(format!("{label}: expected {expected:?}, actual {actual:?}"));
            }
        }
    }
    let compiled = observations
        .iter()
        .filter(|row| row.get("actual").is_some())
        .count();
    let report = json!({"schema":1,"engine_revision":ENGINE_REVISION,"content_version":course.version,
        "kr_compile_observations":compiled,"external_context_descriptions":observations.len()-compiled,
        "references":library.entries.len(),"observations":observations,"failures":failures});
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/reference-fixtures.json");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    assert!(
        failures.is_empty(),
        "{} reference failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

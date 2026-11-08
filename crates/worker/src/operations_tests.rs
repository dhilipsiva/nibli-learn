use super::*;

fn command(runtime: &mut Runtime, command: Command) -> Result<Reply, WorkerError> {
    runtime
        .handle(Request {
            id: "operation-test".into(),
            generation: "7".into(),
            expected_epoch: runtime
                .session
                .as_ref()
                .map(|session| session.epoch.clone()),
            expected_revision: runtime
                .session
                .as_ref()
                .map(|session| session.revision.to_string()),
            command,
        })
        .result
}

fn initialize(runtime: &mut Runtime, source: &str, profile: RunProfile) -> SessionView {
    command(
        runtime,
        Command::InitializeProfile {
            epoch: "main".into(),
            statements: vec![SeedStatement::new(source, "Original source")],
            profile,
            recording: RecordingLevel::Standard,
        },
    )
    .unwrap()
    .session
}

fn execute(runtime: &mut Runtime, operation: Operation) -> OperationRun {
    command(
        runtime,
        Command::Execute {
            operation,
            activity_id: Some("test.activity".into()),
            content_version: 1,
        },
    )
    .unwrap()
    .operation
    .unwrap()
}

fn current(runtime: &mut Runtime) -> SessionView {
    command(runtime, Command::Inspect).unwrap().session
}

fn add(text: &str) -> Change {
    Change::Assert {
        text: text.into(),
        source: "Proposed source".into(),
    }
}

#[test]
fn compile_query_and_admission_failures_remain_distinct() {
    let mut runtime = Runtime::default();
    let before = initialize(
        &mut runtime,
        "derived_only(\"dog\").",
        RunProfile::default(),
    );
    let compile = execute(
        &mut runtime,
        Operation::Compile {
            source: "dog(Adam).".into(),
            structural_preflight: true,
        },
    );
    assert!(matches!(
        compile.result,
        OperationResult::Compiled {
            structural_preflight: StructuralPreflight::Passed,
            ..
        }
    ));
    assert!(compile.proof.is_none());
    let refused = command(
        &mut runtime,
        Command::Change {
            changes: vec![add("dog(Adam).")],
        },
    )
    .unwrap_err();
    assert_eq!(refused.kind, "admission");
    assert_eq!(refused.error_class.as_deref(), Some("reasoning"));
    assert_eq!(refused.stage.as_deref(), Some("change"));
    assert!(refused.record.is_some());
    assert_eq!(current(&mut runtime).recovery, before.recovery);
    let query_only = execute(
        &mut runtime,
        Operation::Compile {
            source: "dog(exactly 1 dog).".into(),
            structural_preflight: true,
        },
    );
    assert!(matches!(
        query_only.result,
        OperationResult::Compiled {
            structural_preflight: StructuralPreflight::Refused { .. },
            ..
        }
    ));
    for operation in [
        Operation::Compile {
            source: "uncommitted(Adam).".into(),
            structural_preflight: false,
        },
        Operation::Decide {
            claim: "uncommitted(Adam).".into(),
        },
    ] {
        let expected_stage = operation.stage().to_string();
        let result = execute(&mut runtime, operation);
        assert!(
            matches!(result.result, OperationResult::Failed { ref stage, ref error_class, .. } if stage == &expected_stage && error_class == "syntax")
        );
        assert!(result.proof.is_none());
        assert!(result.decision().is_none());
        assert!(!result.record.is_null());
    }
    let missing = execute(
        &mut runtime,
        Operation::Decide {
            claim: "dog(Adam).".into(),
        },
    );
    assert!(
        matches!(missing.result, OperationResult::Decision { ref outcome, .. } if outcome.status == "FALSE" && outcome.cwa_false)
    );
    assert!(missing.proof.is_some());
    assert!(missing.decision().is_some());
}

#[test]
fn real_collections_preserve_equality_empty_values_and_typed_unavailability() {
    let mut runtime = Runtime::default();
    let before = initialize(
        &mut runtime,
        "dog(Adam). dog(Bob). Adam = Bob. quantity(Adam, 3). quantity(Cara, 7).",
        RunProfile::default(),
    );
    let answers = execute(
        &mut runtime,
        Operation::Find {
            claim: "dog($x).".into(),
        },
    );
    assert!(
        matches!(answers.result, OperationResult::Answers { ref count, ref rows } if count == "1" && rows.as_array().unwrap().len() == 1)
    );
    assert!(answers.proof.is_none());
    assert!(
        answers.record["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "enumeration_binding")
    );
    let empty = execute(
        &mut runtime,
        Operation::Count {
            claim: "cat($x).".into(),
        },
    );
    assert!(matches!(empty.result, OperationResult::Counted { ref count } if count == "0"));
    for (operator, expected) in [
        (AggregateOperator::Sum, 10.0),
        (AggregateOperator::Min, 3.0),
        (AggregateOperator::Max, 7.0),
        (AggregateOperator::Avg, 5.0),
    ] {
        let result = execute(
            &mut runtime,
            Operation::Aggregate {
                claim: "quantity($x, $n).".into(),
                variable: "$n".into(),
                operator,
            },
        );
        assert!(
            matches!(result.result, OperationResult::Aggregated { value: Some(value), ref witnesses } if value == expected && witnesses == "2")
        );
        assert!(result.proof.is_none());
    }
    let empty = execute(
        &mut runtime,
        Operation::Aggregate {
            claim: "cat($x).".into(),
            variable: "$x".into(),
            operator: AggregateOperator::Sum,
        },
    );
    assert!(
        matches!(empty.result, OperationResult::Aggregated { value: None, ref witnesses } if witnesses == "0")
    );
    for (variable, expected) in [
        ("$missing", "unbound_variable"),
        ("$x", "non_numeric_value"),
    ] {
        let result = execute(
            &mut runtime,
            Operation::Aggregate {
                claim: "quantity($x, $n).".into(),
                variable: variable.into(),
                operator: AggregateOperator::Sum,
            },
        );
        assert!(
            matches!(result.result, OperationResult::CollectionUnavailable { ref kind, .. } if kind == expected)
        );
    }
    for operation in [
        Operation::Find {
            claim: "sum(Adam, 2, 3).".into(),
        },
        Operation::Count {
            claim: "sum(Adam, 2, 3).".into(),
        },
        Operation::Aggregate {
            claim: "sum(Adam, 2, 3).".into(),
            variable: "$x".into(),
            operator: AggregateOperator::Sum,
        },
    ] {
        let result = execute(&mut runtime, operation);
        assert!(
            matches!(result.result, OperationResult::CollectionUnavailable { ref kind, .. } if kind == "incomplete_enumeration")
        );
        assert!(result.proof.is_none());
        assert_eq!(result.record["status"], "complete");
    }
    assert_eq!(current(&mut runtime).recovery, before.recovery);
}

#[test]
fn profiles_are_installed_before_seed_and_revision_tracks_logical_inputs() {
    let mut runtime = Runtime::default();
    let imported = RunProfile {
        existential_import: true,
        ..RunProfile::default()
    };
    let initial = initialize(&mut runtime, "animal(every dog).", imported.clone());
    let found = execute(
        &mut runtime,
        Operation::Find {
            claim: "dog($x).".into(),
        },
    );
    let OperationResult::Answers { rows, .. } = &found.result else {
        panic!("imported witness must be returned")
    };
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .flat_map(|row| row.as_array().unwrap())
            .any(|binding| binding["origin"] == "existential_import")
    );
    let recording_only = command(
        &mut runtime,
        Command::Settings {
            settings: RunSettings {
                recording: RecordingLevel::Full,
                ..initial.settings.clone()
            },
        },
    )
    .unwrap();
    assert_eq!(recording_only.session.revision, initial.revision);
    assert!(found.is_current_main(&recording_only.session));
    let renamed = RunProfile {
        id: "comparison".into(),
        version: 2,
        ..imported
    };
    let metadata_only = command(&mut runtime, Command::SetProfile { profile: renamed }).unwrap();
    assert_eq!(metadata_only.session.revision, initial.revision);
    let clean = command(
        &mut runtime,
        Command::SetProfile {
            profile: RunProfile::default(),
        },
    )
    .unwrap();
    assert_ne!(clean.session.revision, initial.revision);
    assert!(!found.is_current_main(&clean.session));
    assert!(clean.change_record.is_some());
    let empty = execute(
        &mut runtime,
        Operation::Find {
            claim: "dog($x).".into(),
        },
    );
    assert!(matches!(empty.result, OperationResult::Answers { ref count, .. } if count == "0"));
    let before = current(&mut runtime);
    assert!(
        command(
            &mut runtime,
            Command::SetProfile {
                profile: RunProfile {
                    max_chain_depth: 0,
                    ..RunProfile::default()
                }
            }
        )
        .is_err()
    );
    assert_eq!(current(&mut runtime).recovery, before.recovery);
    let mut restored = Runtime::default();
    command(
        &mut restored,
        Command::Restore {
            session: initial.clone(),
        },
    )
    .unwrap();
    assert_eq!(current(&mut restored).profile, initial.profile);
}

#[test]
fn hypotheticals_are_detached_and_apply_readmits_only_the_delta() {
    let mut runtime = Runtime::default();
    let initial = command(
        &mut runtime,
        Command::Initialize {
            epoch: "release-main".into(),
            statements: lesson_setup(1),
            settings: RunSettings::default(),
        },
    )
    .unwrap()
    .session;
    let changes = vec![add("approves(Reviewer, BuildA).")];
    let copy = command(
        &mut runtime,
        Command::Hypothetical {
            candidate_epoch: "candidate".into(),
            source_epoch: initial.epoch.clone(),
            source_revision: initial.revision.clone(),
            changes: changes.clone(),
            profile: Some(RunProfile {
                existential_import: true,
                ..RunProfile::default()
            }),
            operation: Some(Operation::Decide {
                claim: CLAIM.into(),
            }),
            activity_id: Some("what-if".into()),
            content_version: 1,
        },
    )
    .unwrap();
    assert_eq!(copy.session.recovery, initial.recovery);
    let hypothetical = copy.hypothetical.unwrap();
    assert_ne!(hypothetical.candidate.epoch, initial.epoch);
    let run = hypothetical.run.unwrap();
    assert!(
        matches!(run.result, OperationResult::Decision { ref outcome, .. } if outcome.status == "TRUE")
    );
    assert!(!run.is_current_main(&initial));
    assert!(matches!(run.scope, RunScope::Hypothetical { .. }));
    let main = execute(
        &mut runtime,
        Operation::Decide {
            claim: CLAIM.into(),
        },
    );
    assert!(
        matches!(main.result, OperationResult::Decision { ref outcome, .. } if outcome.status == "FALSE")
    );
    let applied = command(
        &mut runtime,
        Command::ApplyDelta {
            source_epoch: initial.epoch.clone(),
            source_revision: initial.revision.clone(),
            changes,
        },
    )
    .unwrap();
    assert_eq!(applied.session.epoch, initial.epoch);
    assert!(
        !applied.session.profile.as_ref().unwrap().existential_import,
        "candidate profile is never transplanted"
    );
    assert!(
        applied
            .session
            .assertions
            .iter()
            .any(|assertion| assertion.text == "approves(Reviewer, BuildA)."
                && assertion.source == "Proposed source")
    );
    assert!(
        applied.change_record.unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "commit" && event["phase"] == "complete")
    );
    let main = execute(
        &mut runtime,
        Operation::Decide {
            claim: CLAIM.into(),
        },
    );
    assert!(
        matches!(main.result, OperationResult::Decision { ref outcome, .. } if outcome.status == "TRUE")
    );
    let before = current(&mut runtime);
    let stale = command(
        &mut runtime,
        Command::ApplyDelta {
            source_epoch: initial.epoch,
            source_revision: initial.revision,
            changes: vec![add("approves(Reviewer, BuildB).")],
        },
    )
    .unwrap_err();
    assert_eq!(stale.kind, "stale");
    assert_eq!(current(&mut runtime).recovery, before.recovery);
    let refused = command(
        &mut runtime,
        Command::ApplyDelta {
            source_epoch: before.epoch.clone(),
            source_revision: before.revision.clone(),
            changes: vec![add("approves(Reviewer, BuildB)."), add(CLAIM)],
        },
    )
    .unwrap_err();
    assert_eq!(refused.kind, "admission");
    assert_eq!(current(&mut runtime).recovery, before.recovery);
}

#[test]
fn project_validation_never_publishes_and_rejects_profile_or_revision_forgery() {
    let mut runtime = Runtime::default();
    let main = initialize(&mut runtime, "dog(Adam).", RunProfile::default());
    let mut legacy = main.clone();
    legacy.epoch = "import-preview".into();
    legacy.profile = None;
    legacy.engine_revision = None;
    legacy.assertions[0].text = "caller presentation is not authoritative".into();
    let reply = command(
        &mut runtime,
        Command::ValidateProject {
            session: legacy.clone(),
        },
    )
    .unwrap();
    assert_eq!(reply.session.epoch, main.epoch);
    assert_eq!(reply.session.recovery, main.recovery);
    let normalized = reply.validated_session.unwrap();
    assert_eq!(normalized.epoch, "import-preview");
    assert!(normalized.profile.is_some());
    assert_eq!(normalized.engine_revision.as_deref(), Some(ENGINE_REVISION));
    assert_eq!(normalized.assertions[0].text, "dog(Adam).");
    let mut invalid = main.clone();
    invalid.profile.as_mut().unwrap().existential_import = true;
    assert!(command(&mut runtime, Command::ValidateProject { session: invalid }).is_err());
    let mut missing_revision = main.clone();
    missing_revision.engine_revision = None;
    assert!(
        command(
            &mut runtime,
            Command::ValidateProject {
                session: missing_revision
            }
        )
        .is_err()
    );
    let mut wrong_engine = main.clone();
    wrong_engine.engine_revision = Some("unrelated-engine".into());
    assert!(
        command(
            &mut runtime,
            Command::Restore {
                session: wrong_engine
            }
        )
        .is_err()
    );
    let mut invalid_source = legacy;
    invalid_source.recovery["engine"]["assertions"][0]["label"] = json!("cat(Adam).");
    assert!(
        command(
            &mut runtime,
            Command::ValidateProject {
                session: invalid_source
            }
        )
        .is_err()
    );
    assert_eq!(current(&mut runtime).recovery, main.recovery);
    assert_eq!(current(&mut runtime).epoch, main.epoch);
    let mut empty = Runtime::default();
    assert!(
        command(
            &mut empty,
            Command::ValidateProject {
                session: normalized
            }
        )
        .is_err()
    );
    assert!(empty.session.is_none());
}

#[test]
fn contradiction_gaps_and_unrecorded_inspections_are_honest() {
    let mut runtime = Runtime::default();
    initialize(
        &mut runtime,
        "dog(Adam). ~dog(Adam). dog(Bob). animal(every dog). alive(every animal). ~alive(Bob).",
        RunProfile {
            max_chain_depth: 1,
            ..RunProfile::default()
        },
    );
    let scan = execute(&mut runtime, Operation::Contradictions);
    assert!(
        matches!(scan.result, OperationResult::Contradictions { ref violations, ref unresolved, completed: false } if !violations.is_empty() && !unresolved.as_array().unwrap().is_empty())
    );
    assert!(scan.proof.is_none());
    assert!(
        scan.record["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "constraint_gap")
    );
    for view in [
        InspectionView::Stratification,
        InspectionView::Materialization,
        InspectionView::AdmissionRegistry,
    ] {
        let inspection = execute(&mut runtime, Operation::Inspect { view });
        assert!(matches!(
            inspection.result,
            OperationResult::Inspection { .. }
        ));
        assert!(inspection.record.is_null());
        assert!(inspection.proof.is_none());
    }
}

#[test]
fn failed_candidates_and_initialization_leave_the_acknowledged_main_unchanged() {
    let mut runtime = Runtime::default();
    let main = command(
        &mut runtime,
        Command::Initialize {
            epoch: "release-main".into(),
            statements: lesson_setup(1),
            settings: RunSettings::default(),
        },
    )
    .unwrap()
    .session;
    for candidate_epoch in ["candidate", "release-main"] {
        let refused = command(
            &mut runtime,
            Command::Hypothetical {
                candidate_epoch: candidate_epoch.into(),
                source_epoch: main.epoch.clone(),
                source_revision: main.revision.clone(),
                changes: vec![add("approves(Reviewer, BuildA)."), add(CLAIM)],
                profile: None,
                operation: Some(Operation::Decide {
                    claim: CLAIM.into(),
                }),
                activity_id: None,
                content_version: 1,
            },
        );
        assert!(refused.is_err());
        assert_eq!(current(&mut runtime).recovery, main.recovery);
        assert_eq!(current(&mut runtime).revision, main.revision);
    }
    let refused = command(
        &mut runtime,
        Command::InitializeProfile {
            epoch: "failed-reset".into(),
            statements: vec![SeedStatement::new("uncommitted(Adam).", "Invalid input")],
            profile: RunProfile {
                existential_import: true,
                ..RunProfile::default()
            },
            recording: RecordingLevel::Full,
        },
    )
    .unwrap_err();
    assert_eq!(refused.error_class.as_deref(), Some("syntax"));
    assert_eq!(refused.stage.as_deref(), Some("change"));
    assert_eq!(current(&mut runtime).epoch, main.epoch);
    assert_eq!(current(&mut runtime).recovery, main.recovery);
}

#[test]
fn decide_reports_actual_unknown_and_depth_limit_then_repair() {
    let mut runtime = Runtime::default();
    initialize(
        &mut runtime,
        "dog(Adam). animal(every dog). alive(every animal).",
        RunProfile {
            max_chain_depth: 1,
            ..RunProfile::default()
        },
    );
    let unknown = execute(
        &mut runtime,
        Operation::Decide {
            claim: "sum(Adam, 2, 3).".into(),
        },
    );
    assert!(
        matches!(unknown.result, OperationResult::Decision { ref outcome, .. } if outcome.status == "UNKNOWN" && outcome.detail.as_deref() == Some("backend-unavailable"))
    );
    let limited = execute(
        &mut runtime,
        Operation::Decide {
            claim: "alive(Adam).".into(),
        },
    );
    assert!(
        matches!(limited.result, OperationResult::Decision { ref outcome, .. } if outcome.status == "RESOURCE_EXCEEDED" && outcome.detail.as_deref() == Some("depth"))
    );
    assert_eq!(limited.engine_revision, ENGINE_REVISION);
    assert_eq!(limited.engine_version, ENGINE_VERSION);
    assert_eq!(limited.profile.max_chain_depth, 1);
    assert_eq!(limited.content_version, 1);
    command(
        &mut runtime,
        Command::SetProfile {
            profile: RunProfile::default(),
        },
    )
    .unwrap();
    let repaired = execute(
        &mut runtime,
        Operation::Decide {
            claim: "alive(Adam).".into(),
        },
    );
    assert!(
        matches!(repaired.result, OperationResult::Decision { ref outcome, .. } if outcome.status == "TRUE")
    );
    assert_ne!(limited.revision, repaired.revision);
}

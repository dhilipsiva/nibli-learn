use super::*;

fn command(runtime: &mut Runtime, cmd: Command) -> Result<Reply, WorkerError> {
    runtime
        .handle(Request {
            id: "test".into(),
            generation: "1".into(),
            expected_epoch: runtime.session.as_ref().map(|s| s.epoch.clone()),
            expected_revision: runtime.session.as_ref().map(|s| s.revision.to_string()),
            command: cmd,
        })
        .result
}
fn start(runtime: &mut Runtime) -> Reply {
    command(
        runtime,
        Command::Initialize {
            epoch: "lesson-test".into(),
            statements: lesson_setup(1),
            settings: RunSettings::default(),
        },
    )
    .unwrap()
}
fn add(runtime: &mut Runtime, text: &str, source: &str) -> Reply {
    command(
        runtime,
        Command::Change {
            changes: vec![Change::Assert {
                text: text.into(),
                source: source.into(),
            }],
        },
    )
    .unwrap()
}
fn verdict(runtime: &mut Runtime) -> RunRecord {
    command(
        runtime,
        Command::Run {
            claim: CLAIM.into(),
        },
    )
    .unwrap()
    .run
    .unwrap()
}

#[test]
fn full_release_readiness_journey_and_independent_support() {
    let mut runtime = Runtime::default();
    start(&mut runtime);
    assert_eq!(verdict(&mut runtime).outcome.status, "FALSE");
    add(&mut runtime, "approves(Reviewer, BuildB).", "Wrong build");
    assert_eq!(verdict(&mut runtime).outcome.status, "FALSE");
    let first = add(&mut runtime, "approves(Reviewer, BuildA).", "Review A1")
        .session
        .assertions
        .into_iter()
        .find(|a| a.source == "Review A1")
        .unwrap()
        .id;
    let run = verdict(&mut runtime);
    assert_eq!(run.outcome.status, "TRUE");
    assert!(
        run.proof_text.contains("BuildA is ready for Release"),
        "{}",
        run.proof_text
    );
    assert!(
        run.proof_text.contains("TestRunner approves BuildA"),
        "{}",
        run.proof_text
    );
    assert!(!run.events().is_empty());
    let second = add(&mut runtime, "approves(Reviewer, BuildA).", "Review A2")
        .session
        .assertions
        .into_iter()
        .find(|a| a.source == "Review A2")
        .unwrap()
        .id;
    command(
        &mut runtime,
        Command::Change {
            changes: vec![Change::Withdraw { id: first }],
        },
    )
    .unwrap();
    assert_eq!(verdict(&mut runtime).outcome.status, "TRUE");
    command(
        &mut runtime,
        Command::Change {
            changes: vec![Change::Withdraw { id: second }],
        },
    )
    .unwrap();
    assert_eq!(verdict(&mut runtime).outcome.status, "FALSE");
    let before = command(&mut runtime, Command::Inspect).unwrap().session;
    let error = command(
        &mut runtime,
        Command::Change {
            changes: vec![Change::Assert {
                text: CLAIM.into(),
                source: "Direct claim".into(),
            }],
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, "admission");
    assert!(error.record.is_some());
    let after = command(&mut runtime, Command::Inspect).unwrap().session;
    assert_eq!(before.recovery, after.recovery);
    assert_eq!(before.revision, after.revision);
    assert!(
        command(
            &mut runtime,
            Command::CheckRepair {
                policy: OR_POLICY.into()
            }
        )
        .unwrap()
        .checks
        .iter()
        .any(|c| !c.passed)
    );
    assert!(
        command(
            &mut runtime,
            Command::CheckRepair {
                policy: AND_POLICY.into()
            }
        )
        .unwrap()
        .checks
        .iter()
        .all(|c| c.passed)
    );
    assert_eq!(
        command(&mut runtime, Command::Inspect)
            .unwrap()
            .session
            .recovery,
        after.recovery
    );
}

#[test]
fn fresh_worker_restore_and_failed_change_preserve_session() {
    let mut original = Runtime::default();
    let start = start(&mut original);
    let mut replacement = Runtime::default();
    command(
        &mut replacement,
        Command::Restore {
            session: start.session.clone(),
        },
    )
    .unwrap();
    assert_eq!(verdict(&mut replacement).outcome.status, "FALSE");
    let error = command(
        &mut replacement,
        Command::Change {
            changes: vec![
                Change::Assert {
                    text: "approves(Reviewer, BuildA).".into(),
                    source: "Tentative".into(),
                },
                Change::Assert {
                    text: CLAIM.into(),
                    source: "Refused".into(),
                },
            ],
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, "admission");
    assert_eq!(
        command(&mut replacement, Command::Inspect)
            .unwrap()
            .session
            .recovery,
        start.session.recovery
    );
}

#[test]
fn stale_requests_and_corrupt_imports_cannot_mutate() {
    let mut runtime = Runtime::default();
    let initial = start(&mut runtime);
    let response = runtime.handle(Request {
        id: "stale".into(),
        generation: "1".into(),
        expected_epoch: Some(initial.session.epoch.clone()),
        expected_revision: Some("0".into()),
        command: Command::Change {
            changes: vec![Change::Assert {
                text: "approves(Reviewer, BuildA).".into(),
                source: "Stale".into(),
            }],
        },
    });
    assert_eq!(response.result.unwrap_err().kind, "stale");
    let mut corrupt = initial.session.clone();
    corrupt.recovery["engine"]["schema_version"] = json!(999);
    assert!(command(&mut runtime, Command::Restore { session: corrupt }).is_err());
    assert_eq!(
        command(&mut runtime, Command::Inspect)
            .unwrap()
            .session
            .recovery,
        initial.session.recovery
    );
}

#[test]
fn profile_depth_and_unknown_are_real_engine_outcomes() {
    let mut runtime = Runtime::default();
    start(&mut runtime);
    let unknown = command(
        &mut runtime,
        Command::Run {
            claim: "sum(Adam, 2, 3).".into(),
        },
    )
    .unwrap()
    .run
    .unwrap();
    assert_eq!(unknown.outcome.status, "UNKNOWN");
    assert!(unknown.outcome.detail.is_some());
    let before = command(&mut runtime, Command::Inspect).unwrap().session;
    let changed = command(
        &mut runtime,
        Command::Settings {
            settings: RunSettings {
                depth: 1,
                recording: RecordingLevel::Standard,
            },
        },
    )
    .unwrap();
    assert_ne!(before.revision, changed.session.revision);
    let run = verdict(&mut runtime);
    assert_eq!(run.settings.depth, 1);
    assert_eq!(run.record["profile"]["max_chain_depth"], json!(1));
}

#[test]
fn proof_export_ids_do_not_lose_js_precision() {
    let id = 9_007_199_254_740_993_u64;
    let mut proof = json!({"root":0,"steps":[{"rule":{"asserted":{"assertions":[{"id":id}],"rule_sources":[{"assertion_id":id}]}},"children":[1]}]});
    stringify_proof_identities(&mut proof);
    assert_eq!(
        proof["steps"][0]["rule"]["asserted"]["assertions"][0]["id"],
        json!(id.to_string())
    );
    assert_eq!(proof["steps"][0]["children"][0], json!(1));
}

#[test]
fn a_delayed_withdrawal_cannot_cross_reset_epochs_with_the_same_revision() {
    let mut runtime = Runtime::default();
    let old = start(&mut runtime).session;
    let fresh = command(
        &mut runtime,
        Command::Initialize {
            epoch: "new-lesson".into(),
            statements: lesson_setup(1),
            settings: RunSettings::default(),
        },
    )
    .unwrap()
    .session;
    assert_eq!(old.revision, fresh.revision);
    let delayed = runtime.handle(Request {
        id: "old-withdrawal".into(),
        generation: "1".into(),
        expected_epoch: Some(old.epoch),
        expected_revision: Some(old.revision),
        command: Command::Change {
            changes: vec![Change::Withdraw {
                id: old.assertions[2].id.clone(),
            }],
        },
    });
    assert_eq!(delayed.result.unwrap_err().kind, "stale");
    assert_eq!(
        command(&mut runtime, Command::Inspect)
            .unwrap()
            .session
            .recovery,
        fresh.recovery
    );
}

//! Execute the original curriculum's authored observations against the real worker.
use nibli_learn_model::*;
use nibli_learn_worker::Runtime;
use serde_json::{Value, json};

#[derive(Default)]
struct Fixture {
    runtime: Runtime,
    session: Option<SessionView>,
    serial: u64,
}
impl Fixture {
    fn call(&mut self, command: Command) -> Result<Reply, WorkerError> {
        self.serial += 1;
        let request = Request {
            id: format!("fixture-{}", self.serial),
            generation: "1".into(),
            expected_epoch: self.session.as_ref().map(|s| s.epoch.clone()),
            expected_revision: self.session.as_ref().map(|s| s.revision.clone()),
            command,
        };
        let result = self.runtime.handle(request).result;
        if let Ok(reply) = &result {
            self.session = Some(reply.session.clone());
        }
        result
    }
    fn execute(
        &mut self,
        operation: Operation,
        activity: &str,
        version: u32,
    ) -> Result<OperationResult, WorkerError> {
        let reply = self.call(Command::Execute {
            operation,
            activity_id: Some(activity.into()),
            content_version: version,
        })?;
        Ok(reply
            .operation
            .expect("Execute must return an operation result")
            .result)
    }
    fn current_json(&self) -> Value {
        serde_json::to_value(self.session.as_ref().unwrap()).unwrap()
    }
    fn assert_preserved(&mut self, before: &Value, label: &str, failures: &mut Vec<String>) {
        match self.call(Command::Inspect) {
            Ok(_) if &self.current_json() == before => {}
            result => failures.push(format!(
                "{label}: failed/detached operation changed acknowledged state: {result:?}"
            )),
        }
    }
}

fn observe(
    activity: &str,
    label: &str,
    expected: Option<&ExpectedResult>,
    actual: &OperationResult,
    observations: &mut Vec<Value>,
    failures: &mut Vec<String>,
) {
    let matched = expected.is_some_and(|expected| expected.matches(actual));
    observations.push(json!({"activity":activity,"observation":label,"expected":expected,"actual":actual,"matched":matched}));
    if !matched {
        failures.push(format!(
            "{activity}/{label}: expected {expected:?}, actual {actual:?}"
        ));
    }
}

#[test]
fn every_authored_engine_observation_and_repair_executes() {
    let course =
        Course::parse(include_str!("../../../content/course.json")).expect("valid course registry");
    ReferenceLibrary::parse(include_str!("../../../content/references.json"), &course)
        .expect("valid references");
    let units: std::collections::BTreeSet<_> =
        course.lessons.iter().map(|l| l.unit.as_str()).collect();
    assert_eq!(
        units,
        ["F1", "F2", "F3", "F4", "F5", "F6", "U1", "U2", "U3", "U4"]
            .into_iter()
            .collect()
    );
    let mut failures = vec![];
    let mut observations = vec![];
    for lesson in &course.lessons {
        for activity in &lesson.activities {
            let mut fixture = Fixture::default();
            let setup = fixture.call(Command::InitializeProfile {
                epoch: format!("fixture-{}", activity.id),
                statements: activity.setup.clone(),
                profile: activity.profile.clone(),
                recording: RecordingLevel::Standard,
            });
            if let Err(error) = setup {
                failures.push(format!(
                    "{}: starting assertions must be admissible: {error}",
                    activity.id
                ));
                continue;
            }
            if let Some(operation) = &activity.operation {
                let actual = fixture
                    .execute(operation.clone(), &activity.id, course.version)
                    .unwrap_or_else(|error| error.as_operation_failure(operation.stage()));
                observe(
                    &activity.id,
                    "initial",
                    activity.expected.as_ref(),
                    &actual,
                    &mut observations,
                    &mut failures,
                );
            }
            for action in &activity.actions {
                let before = fixture.current_json();
                let changes = match resolve_activity_changes(
                    &action.changes,
                    fixture.session.as_ref().unwrap(),
                ) {
                    Ok(changes) => changes,
                    Err(error) => {
                        failures.push(format!(
                            "{}/{}: invalid authored source action: {error}",
                            activity.id, action.id
                        ));
                        continue;
                    }
                };
                if action.scope == ActionScope::Hypothetical {
                    let main = fixture.session.as_ref().unwrap();
                    let result = fixture.call(Command::Hypothetical {
                        candidate_epoch: format!("copy-{}-{}", activity.id, action.id),
                        source_epoch: main.epoch.clone(),
                        source_revision: main.revision.clone(),
                        changes,
                        profile: action.profile.clone(),
                        operation: action.operation.clone(),
                        activity_id: Some(activity.id.clone()),
                        content_version: course.version,
                    });
                    match result {
                        Ok(reply) => {
                            let copy = reply.hypothetical.expect("detached candidate returned");
                            assert_ne!(copy.candidate.epoch, reply.session.epoch);
                            if let Some(run) = copy.run {
                                assert!(matches!(run.scope, RunScope::Hypothetical { .. }));
                                observe(
                                    &activity.id,
                                    &action.id,
                                    action.expected.as_ref(),
                                    &run.result,
                                    &mut observations,
                                    &mut failures,
                                );
                            } else if action.expected.is_some() {
                                failures.push(format!(
                                    "{}/{}: expected result is missing",
                                    activity.id, action.id
                                ));
                            }
                        }
                        Err(error) => observe(
                            &activity.id,
                            &action.id,
                            action.expected.as_ref(),
                            &error.as_operation_failure("change"),
                            &mut observations,
                            &mut failures,
                        ),
                    }
                    fixture.assert_preserved(
                        &before,
                        &format!("{}/{}", activity.id, action.id),
                        &mut failures,
                    );
                    continue;
                }
                if let Some(profile) = &action.profile
                    && let Err(error) = fixture.call(Command::SetProfile {
                        profile: profile.clone(),
                    })
                {
                    observe(
                        &activity.id,
                        &action.id,
                        action.expected.as_ref(),
                        &error.as_operation_failure("profile"),
                        &mut observations,
                        &mut failures,
                    );
                    fixture.assert_preserved(&before, &action.id, &mut failures);
                    continue;
                }
                if !changes.is_empty() {
                    let before_changes = fixture.current_json();
                    if let Err(error) = fixture.call(Command::Change { changes }) {
                        if action.operation.is_some() {
                            failures.push(format!(
                                "{}/{}: changes must succeed before running: {error}",
                                activity.id, action.id
                            ));
                        }
                        observe(
                            &activity.id,
                            &action.id,
                            action.expected.as_ref(),
                            &error.as_operation_failure("change"),
                            &mut observations,
                            &mut failures,
                        );
                        fixture.assert_preserved(&before_changes, &action.id, &mut failures);
                        continue;
                    }
                }
                if let Some(operation) = &action.operation {
                    let actual = fixture
                        .execute(operation.clone(), &activity.id, course.version)
                        .unwrap_or_else(|error| error.as_operation_failure(operation.stage()));
                    observe(
                        &activity.id,
                        &action.id,
                        action.expected.as_ref(),
                        &actual,
                        &mut observations,
                        &mut failures,
                    );
                } else if action.expected.is_some() {
                    failures.push(format!(
                        "{}/{}: expected failure did not occur",
                        activity.id, action.id
                    ));
                }
            }
        }
    }
    let report = json!({"schema":1,"engine_revision":ENGINE_REVISION,"content_version":course.version,
        "activities":course.lessons.iter().map(|l| l.activities.len()).sum::<usize>(),
        "observations":observations,"failures":failures});
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/curriculum-fixtures.json");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    assert!(
        failures.is_empty(),
        "{} curriculum failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

//! Real engine operations in an isolated worker. No lesson result is simulated.
use nibli_learn_model::*;
use nibli_session::CoreSession;
use nibli_types::{
    error::NibliError,
    execution::{ExecutionObserverHandle, RecordingOptions, SessionChange, SessionSnapshot},
    logic::AssertionStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};
use wasm_bindgen::prelude::*;

// Reviewed display frames for the original release exercise. These only phrase
// already-derived proof facts; they do not change parsing, evidence, or reasoning.
static RELEASE_GLOSS: nibli_render::DomainGloss = nibli_render::DomainGloss {
    templates: &[
        ("approves", "{x1} approves {x2}"),
        ("ready", "{x1} is ready for {x2}"),
    ],
    glosses: &[],
    names: &[
        ("builda", "BuildA"),
        ("buildb", "BuildB"),
        ("testrunner", "TestRunner"),
        ("reviewer", "Reviewer"),
        ("release", "Release"),
    ],
};

fn fail(kind: &str, message: impl Into<String>) -> WorkerError {
    WorkerError {
        kind: kind.into(),
        message: message.into(),
        record: None,
    }
}
fn engine_error(error: NibliError) -> WorkerError {
    let kind = match &error {
        NibliError::Syntax(_) => "parse",
        NibliError::Semantic(_) => "validation",
        NibliError::Reasoning(_) => "admission",
        NibliError::Backend(_) => "operational",
    };
    fail(kind, error.to_string())
}
fn recording(level: RecordingLevel) -> RecordingOptions {
    match level {
        RecordingLevel::Standard => RecordingOptions::standard(),
        RecordingLevel::Full => RecordingOptions::full(),
    }
}
fn decimal(value: &str, field: &str) -> Result<u64, WorkerError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| fail("validation", format!("Invalid {field}.")))?;
    if parsed.to_string() != value {
        return Err(fail("validation", format!("Noncanonical {field}.")));
    }
    Ok(parsed)
}

#[cfg(target_arch = "wasm32")]
fn phase_observer(request: &Request) -> Option<ExecutionObserverHandle> {
    use nibli_types::execution::{
        CompilationArtifact, ExecutionEvent, ExecutionObserver, ExecutionPhase,
    };
    struct Notify {
        id: String,
        generation: String,
    }
    impl ExecutionObserver for Notify {
        fn observe(&self, _: ExecutionEvent) {}
        fn compilation(&self, _: CompilationArtifact) {}
        fn phase(&self, phase: ExecutionPhase) {
            use wasm_bindgen::JsCast;
            let notice =
                json!({"kind":"phase", "id":self.id, "generation":self.generation, "phase":phase});
            let worker: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
            // Informational stage entry only. It never acknowledges a state change or verdict.
            let _ = worker.post_message(&JsValue::from_str(&notice.to_string()));
        }
    }
    Some(std::sync::Arc::new(Notify {
        id: request.id.clone(),
        generation: request.generation.clone(),
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn phase_observer(_: &Request) -> Option<ExecutionObserverHandle> {
    None
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceInfo {
    text: String,
    source: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    schema: u32,
    engine: SessionSnapshot,
    sources: BTreeMap<String, SourceInfo>,
}

struct Session {
    core: CoreSession,
    epoch: String,
    revision: u64,
    settings: RunSettings,
    sources: BTreeMap<String, SourceInfo>,
}
impl Session {
    fn initialize(
        epoch: String,
        statements: Vec<SeedStatement>,
        settings: RunSettings,
        observer: Option<ExecutionObserverHandle>,
    ) -> Result<Self, WorkerError> {
        if epoch.is_empty() || epoch.len() > 128 {
            return Err(fail(
                "validation",
                "Session identity must contain 1–128 bytes.",
            ));
        }
        if settings.depth == 0 {
            return Err(fail("validation", "Depth must be a positive integer."));
        }
        let core = CoreSession::new();
        // The teaching profile exposes demand-driven search rather than hidden materialized reuse.
        core.set_materialization(false);
        core.set_strict(false);
        core.set_existential_import(false).map_err(engine_error)?;
        core.set_max_chain_depth(settings.depth)
            .map_err(engine_error)?;
        let mut session = Self {
            core,
            epoch,
            revision: 0,
            settings,
            sources: BTreeMap::new(),
        };
        let changes = statements
            .into_iter()
            .map(|s| Change::Assert {
                text: s.text,
                source: s.source,
            })
            .collect::<Vec<_>>();
        session.change(&changes, observer)?;
        Ok(session)
    }

    fn restore(view: SessionView) -> Result<Self, WorkerError> {
        if view.epoch.is_empty() || view.epoch.len() > 128 {
            return Err(fail("validation", "Invalid session identity."));
        }
        let revision = decimal(&view.revision, "revision")?;
        let recovery: Recovery = serde_json::from_value(view.recovery)
            .map_err(|e| fail("validation", format!("Invalid recovery image: {e}")))?;
        if recovery.schema != SCHEMA_VERSION
            || view.settings.depth != recovery.engine.profile.max_chain_depth
        {
            return Err(fail(
                "validation",
                "Recovery schema or depth does not match this session.",
            ));
        }
        if recovery.engine.profile.existential_import
            || recovery.engine.profile.materialization
            || recovery.engine.profile.strict
        {
            return Err(fail(
                "validation",
                "This lesson profile requires existential import, materialization and strict mode off.",
            ));
        }
        let core = CoreSession::from_snapshot(&recovery.engine)
            .map_err(|e| fail("validation", format!("Recovery image was rejected: {e}")))?;
        let records = core.list_assertion_records().map_err(engine_error)?;
        for (id, source) in &recovery.sources {
            let value = decimal(id, "assertion identity")?;
            let Some(record) = records.iter().find(|r| r.id == value) else {
                return Err(fail("validation", "Source references an absent assertion."));
            };
            if source.text != record.label || source.source.len() > 4096 {
                return Err(fail(
                    "validation",
                    "Source text does not match the engine assertion.",
                ));
            }
        }
        // The caller-supplied presentation list is never an authoritative KB.
        Ok(Self {
            core,
            epoch: view.epoch,
            revision,
            settings: view.settings,
            sources: recovery.sources,
        })
    }

    fn view(&self) -> Result<SessionView, WorkerError> {
        let assertions = self
            .core
            .list_assertion_records()
            .map_err(engine_error)?
            .into_iter()
            .map(|record| {
                let id = record.id.to_string();
                let text = record.label;
                let plain = nibli_session::compile_unmarked(&text).ok().map(|buffer| {
                    nibli_render::render_logic_buffer_with_source(
                        &buffer,
                        nibli_render::Register::Spec,
                        &text,
                    )
                });
                Assertion {
                    source: self
                        .sources
                        .get(&id)
                        .map(|s| s.source.clone())
                        .unwrap_or_default(),
                    id,
                    text,
                    plain,
                    active: record.status == AssertionStatus::Active,
                }
            })
            .collect();
        let recovery = Recovery {
            schema: SCHEMA_VERSION,
            engine: self.core.snapshot().map_err(engine_error)?,
            sources: self.sources.clone(),
        };
        Ok(SessionView {
            epoch: self.epoch.clone(),
            revision: self.revision.to_string(),
            settings: self.settings.clone(),
            assertions,
            recovery: serde_json::to_value(recovery)
                .map_err(|e| fail("serialization", e.to_string()))?,
        })
    }

    fn change(
        &mut self,
        changes: &[Change],
        observer: Option<ExecutionObserverHandle>,
    ) -> Result<Value, WorkerError> {
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| fail("validation", "Revision identity exhausted."))?;
        if changes.len() > 10_000 {
            return Err(fail("validation", "Too many changes in one operation."));
        }
        let engine_changes = changes
            .iter()
            .map(|c| match c {
                Change::Assert { text, source } => {
                    if text.len() > 1024 * 1024 || source.len() > 4096 {
                        return Err(fail(
                            "validation",
                            "Statement or source exceeds the size limit.",
                        ));
                    }
                    Ok(SessionChange::Assert { text: text.clone() })
                }
                Change::Withdraw { id } => {
                    decimal(id, "assertion identity")?;
                    Ok(SessionChange::Withdraw { id: id.clone() })
                }
            })
            .collect::<Result<Vec<_>, WorkerError>>()?;
        let observed = match observer {
            Some(observer) => self.core.apply_changes_recorded_with_phase_observer(
                &engine_changes,
                recording(self.settings.recording),
                observer,
            ),
            None => self
                .core
                .apply_changes_recorded(&engine_changes, recording(self.settings.recording)),
        };
        let record = serde_json::to_value(&observed.record)
            .map_err(|e| fail("serialization", e.to_string()))?;
        let result = observed.outcome.map_err(|error| {
            let mut error = engine_error(error);
            error.record = Some(record.clone());
            error
        })?;
        for (change, ids) in changes.iter().zip(result.operation_ids) {
            if let Change::Assert { text, source } = change {
                for id in ids {
                    self.sources.insert(
                        id,
                        SourceInfo {
                            text: text.clone(),
                            source: source.clone(),
                        },
                    );
                }
            }
        }
        self.revision = next_revision;
        Ok(record)
    }

    fn run(
        &self,
        claim: String,
        run_id: String,
        observer: Option<ExecutionObserverHandle>,
    ) -> Result<RunRecord, WorkerError> {
        if claim.len() > 1024 * 1024 {
            return Err(fail("validation", "Claim exceeds the size limit."));
        }
        let observed = match observer {
            Some(observer) => self.core.query_text_recorded_with_phase_observer(
                &claim,
                recording(self.settings.recording),
                observer,
            ),
            None => self
                .core
                .query_text_recorded(&claim, recording(self.settings.recording)),
        };
        let record = serde_json::to_value(&observed.record)
            .map_err(|e| fail("serialization", e.to_string()))?;
        let (result, trace) = observed.outcome.map_err(|e| {
            let mut e = engine_error(e);
            e.record = Some(record.clone());
            e
        })?;
        let mut proof =
            serde_json::to_value(&trace).map_err(|e| fail("serialization", e.to_string()))?;
        stringify_proof_identities(&mut proof);
        Ok(RunRecord {
            id: run_id,
            epoch: self.epoch.clone(),
            revision: self.revision.to_string(),
            claim,
            settings: self.settings.clone(),
            outcome: Outcome {
                status: result.status_label().into(),
                detail: result.detail_label().map(str::to_owned),
                cwa_false: trace.cwa_false,
                naf_dependent: trace.naf_dependent,
            },
            why: (if result.is_true() {
                nibli_render::summarize_proof_with(
                    &trace,
                    nibli_render::Register::Spec,
                    Some(&RELEASE_GLOSS),
                )
            } else {
                nibli_render::summarize_verdict(
                    &nibli_render::verdict_kind(&result),
                    &trace,
                    nibli_render::Register::Spec,
                )
            })
            .unwrap_or_else(|| "Inspect the engine proof for this result.".into()),
            proof_text: nibli_render::render_collapsed_text_with(
                &trace,
                nibli_render::Register::Spec,
                0,
                false,
                Some(&RELEASE_GLOSS),
            ),
            proof,
            record,
            collapsed_proof: rendered_node_json(&nibli_render::collapse_proof_with(
                &trace,
                nibli_render::Register::Spec,
                Some(&RELEASE_GLOSS),
            )),
        })
    }
}

fn rendered_node_json(node: &nibli_render::RenderedNode) -> Value {
    json!({
        "icon": node.icon, "label": node.label, "css_class": node.css_class,
        "holds": node.holds, "is_leaf": node.is_leaf, "inline": node.inline,
        "children": node.children.iter().map(rendered_node_json).collect::<Vec<_>>()
    })
}

/// App proof exports preserve full assertion identities; graph indices stay u32.
fn stringify_proof_identities(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "id" | "assertion_id") {
                    if let Some(n) = value.as_u64() {
                        *value = Value::String(n.to_string());
                    }
                } else {
                    stringify_proof_identities(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(stringify_proof_identities),
        _ => {}
    }
}

#[derive(Default)]
pub struct Runtime {
    session: Option<Session>,
}
impl Runtime {
    pub fn handle(&mut self, request: Request) -> Response {
        let result = self.execute(&request);
        Response {
            id: request.id,
            generation: request.generation,
            result,
        }
    }
    fn execute(&mut self, request: &Request) -> Result<Reply, WorkerError> {
        if request.id.len() > 256 || request.generation.len() > 32 {
            return Err(fail("validation", "Invalid request identity."));
        }
        if !matches!(
            request.command,
            Command::Initialize { .. } | Command::Restore { .. }
        ) {
            let current = self
                .session
                .as_ref()
                .ok_or_else(|| fail("operational", "Initialize a lesson first."))?;
            if request.expected_epoch.as_deref() != Some(current.epoch.as_str())
                || request.expected_revision.as_deref()
                    != Some(current.revision.to_string().as_str())
            {
                return Err(fail(
                    "stale",
                    "The session epoch or input revision changed. Inspect the current session and retry.",
                ));
            }
        }
        let observer = phase_observer(request);
        let mut run = None;
        let mut change_record = None;
        let mut checks = vec![];
        match request.command.clone() {
            Command::Initialize {
                epoch,
                statements,
                settings,
            } => {
                let candidate = Session::initialize(epoch, statements, settings, observer.clone())?;
                self.session = Some(candidate);
            }
            Command::Restore { session } => {
                let candidate = Session::restore(session)?;
                self.session = Some(candidate);
            }
            command => {
                let session = self
                    .session
                    .as_mut()
                    .ok_or_else(|| fail("operational", "Initialize a lesson first."))?;
                let run_id = format!(
                    "{}:{}:{}:{}",
                    session.epoch, session.revision, request.generation, request.id
                );
                match command {
                    Command::Change { changes } => {
                        change_record = Some(session.change(&changes, observer.clone())?)
                    }
                    Command::Settings { settings } => {
                        if settings.depth == 0 {
                            return Err(fail("validation", "Depth must be positive."));
                        }
                        let next = session
                            .revision
                            .checked_add(1)
                            .ok_or_else(|| fail("validation", "Revision exhausted."))?;
                        session
                            .core
                            .set_max_chain_depth(settings.depth)
                            .map_err(engine_error)?;
                        session.settings = settings;
                        session.revision = next;
                    }
                    Command::Run { claim } => {
                        run = Some(session.run(claim, run_id, observer.clone())?)
                    }
                    Command::CheckRepair { policy } => {
                        let cases = [
                            (
                                "Checker only",
                                vec!["approves(TestRunner, BuildA)."],
                                "FALSE",
                            ),
                            (
                                "Reviewer only",
                                vec!["approves(Reviewer, BuildA)."],
                                "FALSE",
                            ),
                            (
                                "Different builds",
                                vec![
                                    "approves(TestRunner, BuildA).",
                                    "approves(Reviewer, BuildB).",
                                ],
                                "FALSE",
                            ),
                            (
                                "Same build, both approvals",
                                vec![
                                    "approves(TestRunner, BuildA).",
                                    "approves(Reviewer, BuildA).",
                                ],
                                "TRUE",
                            ),
                        ];
                        for (index, (name, facts, expected)) in cases.into_iter().enumerate() {
                            let mut statements = vec![
                                SeedStatement::new(
                                    "derived_only(\"ready\").",
                                    "Protected readiness",
                                ),
                                SeedStatement::new(&policy, "Learner's proposed policy"),
                            ];
                            statements.extend(
                                facts
                                    .into_iter()
                                    .map(|text| SeedStatement::new(text, "Repair case evidence")),
                            );
                            let candidate = Session::initialize(
                                format!("repair-{index}-{}", request.id),
                                statements,
                                session.settings.clone(),
                                observer.clone(),
                            )?;
                            let observed = candidate.run(
                                CLAIM.into(),
                                format!("{run_id}:case:{index}"),
                                observer.clone(),
                            )?;
                            checks.push(CaseCheck {
                                name: name.into(),
                                expected: expected.into(),
                                actual: observed.outcome.status.clone(),
                                passed: observed.outcome.status == expected,
                                run: observed,
                            });
                        }
                    }
                    Command::Inspect => {}
                    Command::Initialize { .. } | Command::Restore { .. } => unreachable!(),
                }
            }
        }
        Ok(Reply {
            session: self.session.as_ref().expect("initialized command").view()?,
            run,
            change_record,
            checks,
        })
    }
}

thread_local! { static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime::default()); }

#[wasm_bindgen]
pub fn handle_request(text: &str) -> String {
    if text.len() > 16 * 1024 * 1024 {
        return json!({"id":"invalid","generation":"invalid","result":{"Err":{"kind":"validation","message":"Request exceeds 16 MiB."}}}).to_string();
    }
    match serde_json::from_str::<Request>(text) {
        Ok(request) => RUNTIME.with(|state| serde_json::to_string(&state.borrow_mut().handle(request)).expect("wire response serializes")),
        Err(error) => json!({"id":"invalid","generation":"invalid","result":{"Err":{"kind":"validation","message":format!("Invalid request: {error}")}}}).to_string(),
    }
}

#[cfg(test)]
mod tests;

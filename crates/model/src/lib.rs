//! Browser-independent lesson, session and worker wire contracts.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;
pub const LESSON_ID: &str = "found.investigate-release";
pub const LESSON_VERSION: u32 = 1;
pub const CLAIM: &str = "ready(BuildA, Release).";
pub const AND_POLICY: &str = "all $build: approves(TestRunner, $build) & approves(Reviewer, $build) -> ready($build, Release).";
pub const OR_POLICY: &str = "all $build: approves(TestRunner, $build) | approves(Reviewer, $build) -> ready($build, Release).";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingLevel {
    #[default]
    Standard,
    Full,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSettings {
    pub depth: u32,
    pub recording: RecordingLevel,
}
impl Default for RunSettings {
    fn default() -> Self {
        Self {
            depth: 10,
            recording: RecordingLevel::Standard,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedStatement {
    pub text: String,
    pub source: String,
}
impl SeedStatement {
    pub fn new(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            source: source.into(),
        }
    }
}

/// Fresh authored setups. Selecting an out-of-order step creates a new epoch.
pub fn lesson_setup(step: usize) -> Vec<SeedStatement> {
    let mut rows = vec![
        SeedStatement::new(
            "derived_only(\"ready\").",
            "Protected readiness declaration",
        ),
        SeedStatement::new(AND_POLICY, "Release policy"),
        SeedStatement::new("approves(TestRunner, BuildA).", "Automated check A1"),
    ];
    if step >= 3 {
        rows.push(SeedStatement::new(
            "approves(Reviewer, BuildB).",
            "Review of another build",
        ));
    }
    if (4..=5).contains(&step) {
        rows.push(SeedStatement::new(
            "approves(Reviewer, BuildA).",
            "Review A1",
        ));
    }
    rows
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub id: String,
    pub text: String,
    pub plain: Option<String>,
    pub source: String,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionView {
    pub epoch: String,
    pub revision: String,
    pub settings: RunSettings,
    pub assertions: Vec<Assertion>,
    /// Versioned engine image; public project import revalidates this boundary.
    pub recovery: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub status: String,
    pub detail: Option<String>,
    pub cwa_false: bool,
    pub naf_dependent: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub epoch: String,
    pub revision: String,
    pub claim: String,
    pub settings: RunSettings,
    pub outcome: Outcome,
    pub why: String,
    pub proof_text: String,
    pub collapsed_proof: Value,
    pub proof: Value,
    /// Actual engine ExecutionRecord, not a UI-generated history.
    pub record: Value,
}
impl RunRecord {
    pub fn is_current(&self, session: &SessionView) -> bool {
        self.epoch == session.epoch && self.revision == session.revision
    }
    pub fn events(&self) -> &[Value] {
        self.record
            .get("events")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    pub fn recording_status(&self) -> &str {
        self.record
            .get("status")
            .or_else(|| self.record.get("recording_status"))
            .and_then(Value::as_str)
            .unwrap_or("unavailable")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Assert { text: String, source: String },
    Withdraw { id: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    Initialize {
        epoch: String,
        statements: Vec<SeedStatement>,
        settings: RunSettings,
    },
    Restore {
        session: SessionView,
    },
    Change {
        changes: Vec<Change>,
    },
    Settings {
        settings: RunSettings,
    },
    Run {
        claim: String,
    },
    CheckRepair {
        policy: String,
    },
    Inspect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub generation: String,
    pub expected_epoch: Option<String>,
    pub expected_revision: Option<String>,
    pub command: Command,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseCheck {
    pub name: String,
    pub expected: String,
    pub actual: String,
    pub passed: bool,
    pub run: RunRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reply {
    pub session: SessionView,
    pub run: Option<RunRecord>,
    pub change_record: Option<Value>,
    pub checks: Vec<CaseCheck>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerError {
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<Value>,
}
impl std::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for WorkerError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub generation: String,
    pub result: Result<Reply, WorkerError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepProgress {
    pub predicted: Option<String>,
    pub checked: bool,
    pub hint_revealed: bool,
    pub solution_revealed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub schema: u32,
    pub lesson: String,
    pub lesson_version: u32,
    pub current_step: usize,
    pub steps: Vec<StepProgress>,
}
impl Default for Progress {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            lesson: LESSON_ID.into(),
            lesson_version: LESSON_VERSION,
            current_step: 1,
            steps: vec![StepProgress::default(); 7],
        }
    }
}
impl Progress {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA_VERSION
            || self.lesson != LESSON_ID
            || self.lesson_version != LESSON_VERSION
        {
            return Err("This progress belongs to an incompatible schema or lesson version. Existing progress was retained.".into());
        }
        if !(1..=7).contains(&self.current_step) || self.steps.len() != 7 {
            return Err(
                "Invalid lesson progress structure. Existing progress was retained.".into(),
            );
        }
        if self.steps.iter().any(|s| {
            s.predicted
                .as_deref()
                .is_some_and(|p| !matches!(p, "TRUE" | "FALSE" | "UNKNOWN" | "RESOURCE_EXCEEDED"))
        }) {
            return Err("Invalid prediction value. Existing progress was retained.".into());
        }
        Ok(())
    }
    pub fn import(text: &str) -> Result<Self, String> {
        if text.len() > 1024 * 1024 {
            return Err("Progress file exceeds 1 MiB.".into());
        }
        let value: Self =
            serde_json::from_str(text).map_err(|e| format!("Invalid progress file: {e}"))?;
        value.validate()?;
        Ok(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectExport {
    pub schema: u32,
    pub kind: String,
    pub session: SessionView,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_or_newer_progress_is_refused() {
        let mut p = Progress::default();
        p.lesson_version += 1;
        assert!(Progress::import(&serde_json::to_string(&p).unwrap()).is_err());
        assert!(Progress::import("{}").is_err());
    }
    #[test]
    fn authored_first_step_has_checker_but_no_review() {
        let setup = lesson_setup(1);
        assert!(
            setup
                .iter()
                .any(|r| r.text == "approves(TestRunner, BuildA).")
        );
        assert!(
            !setup
                .iter()
                .any(|r| r.text.starts_with("approves(Reviewer"))
        );
    }
}

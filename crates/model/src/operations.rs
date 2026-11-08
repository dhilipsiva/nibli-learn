//! Versioned operation results. Non-Boolean answers never borrow a Boolean outcome.
use crate::{Change, Outcome, RecordingLevel, RunRecord, RunSettings, SessionView};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const OPERATION_SCHEMA: u32 = 2;
pub const ENGINE_REVISION: &str = env!("NIBLI_ENGINE_REVISION");
pub const ENGINE_VERSION: &str = env!("NIBLI_ENGINE_VERSION");
pub const APP_REVISION: &str = env!("NIBLI_APP_REVISION");
pub const RUNTIME_ID: &str = env!("NIBLI_RUNTIME_ID");
pub fn runtime_asset_url(path: &str) -> String {
    if RUNTIME_ID.is_empty() {
        format!("/nibli-learn/{}", path.trim_start_matches('/'))
    } else {
        format!(
            "/nibli-learn/runtime/{RUNTIME_ID}/{}",
            path.trim_start_matches('/')
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunProfile {
    pub id: String,
    pub version: u32,
    pub strict: bool,
    pub existential_import: bool,
    pub materialization: bool,
    pub max_chain_depth: u32,
}
impl Default for RunProfile {
    fn default() -> Self {
        Self {
            id: "teaching".into(),
            version: 1,
            strict: false,
            existential_import: false,
            materialization: false,
            max_chain_depth: 10,
        }
    }
}
impl RunProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 128
            || self.version == 0
            || !(1..=256).contains(&self.max_chain_depth)
        {
            return Err("Invalid reasoning profile or depth outside 1–256.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOperator {
    Sum,
    Min,
    Max,
    Avg,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionView {
    Stratification,
    Materialization,
    AdmissionRegistry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Compile {
        source: String,
        #[serde(default)]
        structural_preflight: bool,
    },
    Decide {
        claim: String,
    },
    Find {
        claim: String,
    },
    Count {
        claim: String,
    },
    Aggregate {
        claim: String,
        variable: String,
        operator: AggregateOperator,
    },
    Contradictions,
    Inspect {
        view: InspectionView,
    },
}
impl Operation {
    pub fn source(&self) -> &str {
        match self {
            Self::Compile { source, .. } => source,
            Self::Decide { claim }
            | Self::Find { claim }
            | Self::Count { claim }
            | Self::Aggregate { claim, .. } => claim,
            Self::Contradictions | Self::Inspect { .. } => "",
        }
    }
    pub fn stage(&self) -> &'static str {
        match self {
            Self::Compile { .. } => "compile",
            Self::Decide { .. } => "query",
            Self::Find { .. } => "find",
            Self::Count { .. } => "count",
            Self::Aggregate { .. } => "aggregate",
            Self::Contradictions => "contradictions",
            Self::Inspect { .. } => "inspect",
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.source().len() > 1024 * 1024 {
            return Err("Operation input exceeds 1 MiB.".into());
        }
        if let Self::Aggregate { variable, .. } = self
            && (variable.is_empty() || variable.len() > 256)
        {
            return Err("An aggregate needs a bounded variable name.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StructuralPreflight {
    NotRequested,
    Passed,
    Refused { message: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum OperationResult {
    Compiled {
        logic: Value,
        structural_preflight: StructuralPreflight,
    },
    Decision {
        outcome: Outcome,
        why: String,
    },
    /// Only complete enumeration produces this variant. Rows are actual engine bindings.
    Answers {
        rows: Value,
        count: String,
    },
    Counted {
        count: String,
    },
    Aggregated {
        value: Option<f64>,
        witnesses: String,
    },
    Contradictions {
        violations: Vec<String>,
        unresolved: Value,
        completed: bool,
    },
    Inspection {
        view: InspectionView,
        data: Value,
    },
    CollectionUnavailable {
        kind: String,
        message: String,
    },
    Failed {
        stage: String,
        error_class: String,
        message: String,
    },
}
impl OperationResult {
    pub fn family(&self) -> &'static str {
        match self {
            Self::Compiled { .. } => "compiled",
            Self::Decision { .. } => "decision",
            Self::Answers { .. } => "answers",
            Self::Counted { .. } => "counted",
            Self::Aggregated { .. } => "aggregated",
            Self::Contradictions { .. } => "contradictions",
            Self::Inspection { .. } => "inspection",
            Self::CollectionUnavailable { .. } => "collection_unavailable",
            Self::Failed { .. } => "failed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProofArtifact {
    pub text: String,
    pub collapsed: Value,
    pub graph: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunScope {
    Main,
    Hypothetical {
        source_epoch: String,
        source_revision: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationRun {
    pub schema: u32,
    pub id: String,
    pub request_id: String,
    pub generation: String,
    pub scope: RunScope,
    pub epoch: String,
    pub revision: String,
    pub engine_version: String,
    pub engine_revision: String,
    pub content_version: u32,
    pub activity_id: Option<String>,
    pub profile: RunProfile,
    pub recording: RecordingLevel,
    pub operation: Operation,
    pub result: OperationResult,
    pub proof: Option<ProofArtifact>,
    pub record: Value,
}
impl OperationRun {
    /// Compatibility view for existing Boolean proof/replay components; no new inference.
    pub fn decision(&self) -> Option<RunRecord> {
        let OperationResult::Decision { outcome, why } = &self.result else {
            return None;
        };
        let proof = self.proof.as_ref()?;
        Some(RunRecord {
            id: self.id.clone(),
            epoch: self.epoch.clone(),
            revision: self.revision.clone(),
            claim: self.operation.source().into(),
            settings: RunSettings {
                depth: self.profile.max_chain_depth,
                recording: self.recording,
            },
            outcome: outcome.clone(),
            why: why.clone(),
            proof_text: proof.text.clone(),
            collapsed_proof: proof.collapsed.clone(),
            proof: proof.graph.clone(),
            record: self.record.clone(),
        })
    }
    pub fn is_current_main(&self, session: &SessionView) -> bool {
        self.scope == RunScope::Main
            && self.epoch == session.epoch
            && self.revision == session.revision
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HypotheticalResult {
    pub source_epoch: String,
    pub source_revision: String,
    pub proposed_delta: Vec<Change>,
    pub candidate: SessionView,
    pub profile: RunProfile,
    pub run: Option<OperationRun>,
}

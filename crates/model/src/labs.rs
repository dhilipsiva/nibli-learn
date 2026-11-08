//! Local reports are supplied evidence, never authenticated browser executions.
use crate::{ENGINE_REVISION, text_sha256, valid_content_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
pub const MAX_LAB_REPORT_BYTES: usize = 2 * 1024 * 1024;
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabDefinition {
    pub id: String,
    pub version: u32,
    pub title: String,
    pub command: String,
    pub platform: String,
    pub kit_sha256: String,
    pub required_checks: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabLibrary {
    pub schema: u32,
    pub download: String,
    pub entries: Vec<LabDefinition>,
}
impl LabLibrary {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 1024 * 1024 {
            return Err("Lab library exceeds 1 MiB.".into());
        }
        let library: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if library.schema != 1
            || library.download != "assets/nibli-labs-v1.tar.gz"
            || library.entries.is_empty()
            || library.entries.len() > 32
        {
            return Err("Invalid lab library.".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for lab in &library.entries {
            if !valid_content_id(&lab.id)
                || !ids.insert(&lab.id)
                || lab.version == 0
                || lab.title.trim().is_empty()
                || lab.title.len() > 512
                || lab.command.trim().is_empty()
                || lab.command.len() > 4096
                || lab.platform.trim().is_empty()
                || lab.platform.len() > 1024
                || !hash(&lab.kit_sha256)
                || lab.required_checks.is_empty()
                || lab.required_checks.len() > 64
                || lab
                    .required_checks
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 128)
            {
                return Err("Invalid local lab identity or checks.".into());
            }
        }
        Ok(library)
    }
    pub fn get(&self, id: &str) -> Option<&LabDefinition> {
        self.entries.iter().find(|l| l.id == id)
    }
    pub fn validate_links(
        &self,
        course: &crate::Course,
        references: &crate::ReferenceLibrary,
    ) -> Result<(), String> {
        let activities = course
            .lessons
            .iter()
            .flat_map(|lesson| &lesson.activities)
            .filter_map(|activity| activity.local_lab.as_deref());
        let examples = references
            .entries
            .iter()
            .flat_map(|entry| &entry.examples)
            .filter_map(|example| example.lab.as_deref());
        if activities.chain(examples).any(|id| self.get(id).is_none()) {
            return Err("Course or reference points to an unavailable local lab.".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabReport {
    pub schema: u32,
    pub kind: String,
    pub lab_id: String,
    pub lab_version: u32,
    pub engine_revision: String,
    pub kit_sha256: String,
    pub source_files: BTreeMap<String, String>,
    pub platform: BTreeMap<String, String>,
    pub tools: BTreeMap<String, String>,
    pub execution_claim: String,
    pub status: String,
    #[serde(default)]
    pub failure: Option<String>,
    #[serde(default)]
    pub observations: Value,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
    #[serde(default)]
    pub runs: Vec<Value>,
    pub logs: BTreeMap<String, LabLog>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabLog {
    pub sha256: String,
    pub bytes: u64,
}
impl LabReport {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_LAB_REPORT_BYTES {
            return Err("Local lab report exceeds 2 MiB.".into());
        }
        let report: Self =
            serde_json::from_str(text).map_err(|e| format!("Invalid lab report: {e}"))?;
        report.validate()?;
        Ok(report)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.kind != "nibli-local-lab-report"
            || !valid_content_id(&self.lab_id)
            || self.lab_version == 0
            || !hash(&self.kit_sha256)
            || !matches!(self.status.as_str(), "passed" | "failed")
            || self.execution_claim != "learner-supplied; not authenticated by the companion"
            || self.source_files.is_empty()
            || self.source_files.len() > 256
            || self.logs.is_empty()
            || self.logs.len() > 64
            || self.runs.len() > 64
            || self.platform.len() > 16
            || self.tools.len() > 32
            || self.artifacts.len() > 32
            || self.failure.as_ref().is_some_and(|s| s.len() > 4096)
        {
            return Err("Unsupported or excessive lab report fields.".into());
        }
        if self
            .source_files
            .iter()
            .any(|(k, v)| !safe_path(k) || !hash(v))
            || self
                .logs
                .iter()
                .any(|(k, v)| !safe_path(k) || !hash(&v.sha256) || v.bytes > 1024 * 1024 * 1024)
            || self.artifacts.values().any(|v| !hash(v))
            || self
                .platform
                .iter()
                .chain(&self.tools)
                .any(|(k, v)| k.len() > 128 || v.len() > 4096)
        {
            return Err("Invalid lab source, artifact, log or environment metadata.".into());
        }
        let canonical = serde_json::to_string(&self.source_files).map_err(|e| e.to_string())?;
        if text_sha256(&canonical) != self.kit_sha256 {
            return Err("The kit digest does not match its supplied source inventory.".into());
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_LAB_REPORT_BYTES {
            return Err("Local lab report exceeds 2 MiB.".into());
        }
        Ok(())
    }
    pub fn meets(&self, lab: &LabDefinition) -> bool {
        self.status == "passed"
            && self.failure.is_none()
            && self.engine_revision == ENGINE_REVISION
            && self.lab_id == lab.id
            && self.lab_version == lab.version
            && self.kit_sha256 == lab.kit_sha256
            && !self.artifacts.is_empty()
            && !self.runs.is_empty()
            && self
                .runs
                .iter()
                .all(|run| run["exit_code"].as_i64() == Some(0))
            && ["system", "release", "machine"].iter().all(|key| {
                self.platform
                    .get(*key)
                    .is_some_and(|v| !v.trim().is_empty())
            })
            && ["python", "rustc", "cargo"]
                .iter()
                .all(|key| self.tools.get(*key).is_some_and(|v| !v.trim().is_empty()))
            && lab
                .required_checks
                .iter()
                .all(|check| self.observations["checks"][check].as_bool() == Some(true))
    }
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_content_cannot_reference_an_absent_lab() {
        let mut course =
            crate::Course::parse(include_str!("../../../content/course.json")).unwrap();
        let references = crate::ReferenceLibrary::parse(
            include_str!("../../../content/references.json"),
            &course,
        )
        .unwrap();
        let labs = LabLibrary::parse(include_str!("../../../content/labs.json")).unwrap();
        labs.validate_links(&course, &references).unwrap();
        course.lessons[0].activities[0].local_lab = Some("absent-lab".into());
        assert!(labs.validate_links(&course, &references).is_err());
    }
    fn fixture() -> (LabDefinition, LabReport) {
        let source_files = BTreeMap::from([("native/src/main.rs".into(), text_sha256("fixture"))]);
        let kit = text_sha256(&serde_json::to_string(&source_files).unwrap());
        let lab = LabDefinition {
            id: "compute".into(),
            version: 1,
            title: "Compute".into(),
            command: "run".into(),
            platform: "Linux".into(),
            kit_sha256: kit.clone(),
            required_checks: vec!["observed".into()],
        };
        let report = LabReport {
            schema: 1,
            kind: "nibli-local-lab-report".into(),
            lab_id: "compute".into(),
            lab_version: 1,
            engine_revision: ENGINE_REVISION.into(),
            kit_sha256: kit,
            source_files,
            platform: BTreeMap::from([
                ("system".into(), "Linux".into()),
                ("release".into(), "test fixture".into()),
                ("machine".into(), "x86_64".into()),
            ]),
            tools: BTreeMap::from([
                ("python".into(), "fixture version".into()),
                ("rustc".into(), "fixture version".into()),
                ("cargo".into(), "fixture version".into()),
            ]),
            execution_claim: "learner-supplied; not authenticated by the companion".into(),
            status: "passed".into(),
            failure: None,
            observations: serde_json::json!({"checks":{"observed":true}}),
            artifacts: BTreeMap::from([("binary_sha256".into(), text_sha256("binary"))]),
            runs: vec![serde_json::json!({"exit_code":0})],
            logs: BTreeMap::from([(
                "run.stdout".into(),
                LabLog {
                    sha256: text_sha256("log"),
                    bytes: 3,
                },
            )]),
        };
        (lab, report)
    }
    #[test]
    fn failed_foreign_tampered_and_non_boolean_observations_do_not_meet_a_lab() {
        let (lab, mut report) = fixture();
        report.validate().unwrap();
        assert!(report.meets(&lab));
        report.runs[0]["exit_code"] = serde_json::json!(1);
        assert!(!report.meets(&lab));
        report.runs[0]["exit_code"] = serde_json::json!(0);
        let machine = report.platform.remove("machine").unwrap();
        assert!(!report.meets(&lab));
        report.platform.insert("machine".into(), machine);
        report.observations["checks"]["observed"] = serde_json::json!("true");
        assert!(!report.meets(&lab));
        report.observations["checks"]["observed"] = serde_json::json!(true);
        report.status = "failed".into();
        report.failure = Some("actual experiment failed".into());
        report.validate().unwrap();
        assert!(!report.meets(&lab));
        report.status = "passed".into();
        report.failure = None;
        report.engine_revision = "another-engine".into();
        assert!(!report.meets(&lab));
        report
            .source_files
            .insert("../escape".into(), text_sha256("bad"));
        assert!(report.validate().is_err());
    }
}

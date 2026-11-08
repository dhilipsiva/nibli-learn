//! Declarative, versioned teaching material. Content cannot supply executable code.
use crate::{Operation, OperationResult, RunProfile, SeedStatement};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const COURSE_SCHEMA: u32 = 2;
pub const RELEASE_OBJECTIVES: [&str; 7] = [
    "release.one-approval",
    "release.different-build",
    "release.same-build",
    "release.execution-and-proof",
    "release.independent-support",
    "release.protected-conclusion",
    "release.repair-policy",
];

pub fn valid_content_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Course {
    pub schema: u32,
    pub version: u32,
    pub lessons: Vec<Lesson>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    pub id: String,
    pub version: u32,
    pub unit: String,
    pub track: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default)]
    pub legacy_release: bool,
    pub activities: Vec<Activity>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub id: String,
    pub version: u32,
    pub title: String,
    pub objective: String,
    pub explanation: String,
    pub tasks: Vec<String>,
    pub hints: Vec<String>,
    pub solution: String,
    #[serde(default)]
    pub setup: Vec<SeedStatement>,
    #[serde(default)]
    pub profile: RunProfile,
    #[serde(default)]
    pub operation: Option<Operation>,
    #[serde(default)]
    pub expected: Option<ExpectedResult>,
    #[serde(default)]
    pub question: Option<KnowledgeCheck>,
    #[serde(default)]
    pub actions: Vec<ActivityAction>,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub self_review: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeCheck {
    pub prompt: String,
    pub choices: Vec<String>,
    pub correct: usize,
    pub feedback: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivityChange {
    Assert {
        text: String,
        source: String,
    },
    WithdrawSource {
        source: String,
    },
    ReplaceSource {
        source: String,
        text: String,
        replacement_source: String,
    },
}

/// Resolve authored source labels against this acknowledged session; never guess an ID.
pub fn resolve_activity_changes(
    changes: &[ActivityChange],
    session: &crate::SessionView,
) -> Result<Vec<crate::Change>, String> {
    let mut result = Vec::new();
    for change in changes {
        match change {
            ActivityChange::Assert { text, source } => result.push(crate::Change::Assert {
                text: text.clone(),
                source: source.clone(),
            }),
            ActivityChange::WithdrawSource { source }
            | ActivityChange::ReplaceSource { source, .. } => {
                let rows: Vec<_> = session
                    .assertions
                    .iter()
                    .filter(|a| a.active && &a.source == source)
                    .collect();
                if rows.len() != 1 {
                    return Err(format!(
                        "This action needs exactly one active assertion labeled '{source}'. Restore the activity setup or inspect its preceding actions."
                    ));
                }
                result.push(crate::Change::Withdraw {
                    id: rows[0].id.clone(),
                });
                if let ActivityChange::ReplaceSource {
                    text,
                    replacement_source,
                    ..
                } = change
                {
                    result.push(crate::Change::Assert {
                        text: text.clone(),
                        source: replacement_source.clone(),
                    });
                }
            }
        }
    }
    Ok(result)
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionScope {
    #[default]
    Main,
    Hypothetical,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityAction {
    pub id: String,
    pub title: String,
    pub instruction: String,
    #[serde(default)]
    pub scope: ActionScope,
    #[serde(default)]
    pub changes: Vec<ActivityChange>,
    #[serde(default)]
    pub profile: Option<RunProfile>,
    #[serde(default)]
    pub operation: Option<Operation>,
    #[serde(default)]
    pub expected: Option<ExpectedResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedResult {
    pub family: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub count: Option<String>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub empty: Option<bool>,
    #[serde(default)]
    pub violations: Option<bool>,
    #[serde(default)]
    pub unresolved: Option<bool>,
    #[serde(default)]
    pub error_class: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
}
impl ExpectedResult {
    pub fn matches(&self, actual: &OperationResult) -> bool {
        if self.family != actual.family() {
            return false;
        }
        match actual {
            OperationResult::Decision { outcome, .. } => {
                self.status.as_ref().is_none_or(|s| s == &outcome.status)
                    && self
                        .detail
                        .as_ref()
                        .is_none_or(|d| outcome.detail.as_ref() == Some(d))
            }
            OperationResult::Answers { rows, count } => {
                self.count.as_ref().is_none_or(|n| n == count)
                    && self
                        .empty
                        .is_none_or(|empty| rows.as_array().is_some_and(|r| r.is_empty() == empty))
            }
            OperationResult::Counted { count } => self.count.as_ref().is_none_or(|n| n == count),
            OperationResult::Aggregated { value, witnesses } => {
                self.empty.is_none_or(|empty| value.is_none() == empty)
                    && self.value.is_none_or(|expected| *value == Some(expected))
                    && self.count.as_ref().is_none_or(|n| n == witnesses)
            }
            OperationResult::Contradictions {
                violations,
                unresolved,
                ..
            } => {
                self.violations
                    .is_none_or(|expected| violations.is_empty() != expected)
                    && self.unresolved.is_none_or(|expected| {
                        unresolved
                            .as_array()
                            .is_some_and(|g| g.is_empty() != expected)
                    })
            }
            OperationResult::CollectionUnavailable { kind, .. } => {
                self.detail.as_ref().is_none_or(|d| d == kind)
            }
            OperationResult::Failed {
                stage, error_class, ..
            } => {
                self.stage.as_ref().is_none_or(|s| s == stage)
                    && self.error_class.as_ref().is_none_or(|c| c == error_class)
            }
            OperationResult::Compiled {
                structural_preflight,
                ..
            } => self.status.as_ref().is_none_or(|status| {
                status
                    == match structural_preflight {
                        crate::StructuralPreflight::NotRequested => "not_requested",
                        crate::StructuralPreflight::Passed => "passed",
                        crate::StructuralPreflight::Refused { .. } => "refused",
                    }
            }),
            OperationResult::Inspection { .. } => true,
        }
    }
    fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.family.as_str(),
            "decision"
                | "compiled"
                | "answers"
                | "counted"
                | "aggregated"
                | "contradictions"
                | "inspection"
                | "collection_unavailable"
                | "failed"
        ) || self
            .count
            .as_ref()
            .is_some_and(|n| n.parse::<u64>().ok().is_none_or(|v| v.to_string() != *n))
            || self.value.is_some_and(|value| !value.is_finite())
        {
            return Err("Invalid authored expected result.".into());
        }
        let allowed: &[&str] = match self.family.as_str() {
            "decision" => &["status", "detail"],
            "compiled" => &["status"],
            "answers" => &["count", "empty"],
            "counted" => &["count"],
            "aggregated" => &["count", "value", "empty"],
            "contradictions" => &["violations", "unresolved"],
            "collection_unavailable" => &["detail"],
            "failed" => &["error_class", "stage"],
            _ => &[],
        };
        let fields = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if fields.as_object().unwrap().iter().any(|(key, value)| {
            key != "family" && !value.is_null() && !allowed.contains(&key.as_str())
        }) {
            return Err("An expected field does not apply to this result family.".into());
        }
        if let Some(status) = self.status.as_deref() {
            let statuses: &[&str] = if self.family == "decision" {
                &["TRUE", "FALSE", "UNKNOWN", "RESOURCE_EXCEEDED"]
            } else {
                &["not_requested", "passed", "refused"]
            };
            if !statuses.contains(&status) {
                return Err("Unknown expected result status.".into());
            }
        }
        if self.empty == Some(true) && self.value.is_some() {
            return Err("An empty aggregate cannot have a numeric value.".into());
        }
        Ok(())
    }
}

impl Course {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 8 * 1024 * 1024 {
            return Err("Course exceeds 8 MiB.".into());
        }
        let course: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        course.validate()?;
        Ok(course)
    }
    pub fn lesson(&self, id: &str) -> Option<&Lesson> {
        self.lessons.iter().find(|l| l.id == id)
    }
    pub fn activity(&self, lesson: &str, id: &str) -> Option<&Activity> {
        self.lesson(lesson)?.activities.iter().find(|a| a.id == id)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != COURSE_SCHEMA
            || self.version == 0
            || self.lessons.is_empty()
            || self.lessons.len() > 128
        {
            return Err("Unsupported course schema, version or lesson inventory.".into());
        }
        let mut ids = BTreeSet::new();
        let mut activities = BTreeSet::new();
        for lesson in &self.lessons {
            if !valid_content_id(&lesson.id)
                || !ids.insert(&lesson.id)
                || lesson.version == 0
                || lesson.activities.is_empty()
                || lesson.activities.len() > 128
                || !matches!(
                    lesson.track.as_str(),
                    "foundations" | "use" | "integrate" | "build" | "studio"
                )
            {
                return Err(format!("Invalid or duplicate lesson {}.", lesson.id));
            }
            if lesson.legacy_release
                && (lesson.id != crate::LESSON_ID
                    || lesson.version != 1
                    || lesson
                        .activities
                        .iter()
                        .any(|activity| activity.version != 1)
                    || lesson
                        .activities
                        .iter()
                        .map(|a| a.id.as_str())
                        .collect::<Vec<_>>()
                        != RELEASE_OBJECTIVES)
            {
                return Err(
                    "The original investigation's seven identities must be preserved.".into(),
                );
            }
            for activity in &lesson.activities {
                if !valid_content_id(&activity.id)
                    || !activities.insert(&activity.id)
                    || activity.version == 0
                    || activity.objective.trim().is_empty()
                    || activity.explanation.trim().is_empty()
                    || activity.tasks.is_empty()
                    || activity.hints.is_empty()
                    || activity.solution.trim().is_empty()
                {
                    return Err(format!("Invalid or duplicate activity {}.", activity.id));
                }
                activity.profile.validate()?;
                if let Some(op) = &activity.operation {
                    op.validate()?;
                }
                if let Some(expected) = &activity.expected {
                    expected.validate()?;
                }
                if let Some(question) = &activity.question
                    && (question.choices.len() < 2 || question.correct >= question.choices.len())
                {
                    return Err(format!("Invalid learning question for {}.", activity.id));
                }
                let mut actions = BTreeSet::new();
                for action in &activity.actions {
                    if !valid_content_id(&action.id) || !actions.insert(&action.id) {
                        return Err(format!("Invalid or duplicate action {}.", action.id));
                    }
                    if let Some(profile) = &action.profile {
                        profile.validate()?;
                    }
                    if let Some(op) = &action.operation {
                        op.validate()?;
                    }
                    if let Some(expected) = &action.expected {
                        expected.validate()?;
                    }
                }
            }
        }
        for lesson in &self.lessons {
            if lesson
                .prerequisites
                .iter()
                .any(|id| self.lesson(id).is_none())
            {
                return Err(format!(
                    "Unknown recommended prerequisite in {}.",
                    lesson.id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLibrary {
    pub schema: u32,
    pub version: u32,
    pub entries: Vec<ReferenceEntry>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceEntry {
    pub id: String,
    pub title: String,
    pub category: String,
    pub body: String,
    #[serde(default)]
    pub examples: Vec<ReferenceExample>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub links: Vec<ReferenceLink>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceExample {
    pub source: String,
    pub explanation: String,
    /// Authored compile contract; absent on compatible older reference files.
    #[serde(default)]
    pub expected: Option<ExpectedResult>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLink {
    pub label: String,
    pub url: String,
}
impl ReferenceLibrary {
    pub fn parse(text: &str, course: &Course) -> Result<Self, String> {
        if text.len() > 8 * 1024 * 1024 {
            return Err("Reference library exceeds 8 MiB.".into());
        }
        let library: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if library.schema != COURSE_SCHEMA || library.version == 0 {
            return Err("Unsupported reference version.".into());
        }
        let mut ids = BTreeSet::new();
        for entry in &library.entries {
            for example in &entry.examples {
                if let Some(expected) = &example.expected {
                    expected.validate()?;
                    if !matches!(expected.family.as_str(), "compiled" | "failed") {
                        return Err(format!(
                            "Reference {} expects a non-compile operation.",
                            entry.id
                        ));
                    }
                }
            }
            if !valid_content_id(&entry.id)
                || !ids.insert(entry.id.as_str())
                || entry.body.trim().is_empty()
                || entry
                    .links
                    .iter()
                    .any(|l| !l.url.starts_with("https://") || l.url.contains(char::is_whitespace))
            {
                return Err(format!("Invalid reference {} or external link.", entry.id));
            }
        }
        if library
            .entries
            .iter()
            .any(|entry| entry.related.iter().any(|id| !ids.contains(id.as_str())))
            || course
                .lessons
                .iter()
                .flat_map(|l| &l.activities)
                .any(|a| a.references.iter().any(|id| !ids.contains(id.as_str())))
        {
            return Err("Unknown reference identity.".into());
        }
        Ok(library)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn expected_result_rejects_fields_that_would_be_silently_ignored() {
        for invalid in [
            json!({"family":"decision","status":"TRUE","count":"3"}),
            json!({"family":"compiled","status":"TRUE"}),
            json!({"family":"aggregated","empty":true,"value":0}),
            json!({"family":"inspection","error_class":"syntax"}),
        ] {
            let expected: ExpectedResult = serde_json::from_value(invalid).unwrap();
            assert!(expected.validate().is_err());
        }
    }
}

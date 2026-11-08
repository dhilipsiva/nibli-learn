//! Progress identities survive reordering; older evidence never completes a changed objective.
use crate::{Course, LESSON_ID, Progress, RELEASE_OBJECTIVES, StepProgress, valid_content_id};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PROGRESS_SCHEMA: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningSelection {
    pub lesson: String,
    pub activity: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityProgress {
    pub prediction: Option<String>,
    pub checked: bool,
    pub hints_revealed: usize,
    pub solution_revealed: bool,
    pub self_reviewed: bool,
    /// Schema 1 stored completion flags, not a persistent proof or execution transcript.
    pub legacy: bool,
    #[serde(default)]
    pub actions: BTreeSet<String>,
    #[serde(default)]
    pub evidence: Vec<LearningEvidence>,
    #[serde(default)]
    pub candidate_evidence: Vec<LearningEvidence>,
    #[serde(default)]
    pub expected_failures: Vec<LearningFailureEvidence>,
    #[serde(default)]
    pub answer: Option<usize>,
    #[serde(default)]
    pub reflection: Option<String>,
    #[serde(default)]
    pub reviews: Vec<crate::ReviewLearningEvidence>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningEvidence {
    pub engine_revision: String,
    pub content_version: u32,
    pub run_id: String,
    pub epoch: String,
    pub revision: String,
    pub action: Option<String>,
    pub result_family: String,
    pub outcome: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningFailureEvidence {
    pub engine_revision: String,
    pub content_version: u32,
    pub epoch: String,
    pub revision: String,
    pub action: Option<String>,
    pub stage: String,
    pub error_class: String,
}
pub type ObjectiveVersions = BTreeMap<u32, ActivityProgress>;
pub type LessonActivities = BTreeMap<String, ObjectiveVersions>;
pub type LessonVersions = BTreeMap<u32, LessonActivities>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CourseProgress {
    pub schema: u32,
    pub course_version: u32,
    pub current: LearningSelection,
    pub lessons: BTreeMap<String, LessonVersions>,
}
impl CourseProgress {
    pub fn new(course: &Course) -> Self {
        let lesson = &course.lessons[0];
        Self {
            schema: PROGRESS_SCHEMA,
            course_version: course.version,
            current: LearningSelection {
                lesson: lesson.id.clone(),
                activity: lesson.activities[0].id.clone(),
            },
            lessons: BTreeMap::new(),
        }
    }
    pub fn activity(
        &self,
        course: &Course,
        lesson: &str,
        activity: &str,
    ) -> Option<&ActivityProgress> {
        let definition = course.lesson(lesson)?;
        let objective = course.activity(lesson, activity)?;
        self.lessons
            .get(lesson)?
            .get(&definition.version)?
            .get(activity)?
            .get(&objective.version)
    }
    pub fn activity_mut<'a>(
        &'a mut self,
        course: &Course,
        lesson: &str,
        activity: &str,
    ) -> Result<&'a mut ActivityProgress, String> {
        let definition = course.lesson(lesson).ok_or("Unknown lesson.")?;
        let objective = course
            .activity(lesson, activity)
            .ok_or("Unknown activity.")?;
        Ok(self
            .lessons
            .entry(lesson.into())
            .or_default()
            .entry(definition.version)
            .or_default()
            .entry(activity.into())
            .or_default()
            .entry(objective.version)
            .or_default())
    }
    pub fn completed(&self, course: &Course) -> usize {
        course
            .lessons
            .iter()
            .flat_map(|l| l.activities.iter().map(move |a| (l, a)))
            .filter(|(l, a)| {
                self.activity(course, &l.id, &a.id)
                    .is_some_and(|p| p.checked)
            })
            .count()
    }
    pub fn migrate_legacy(legacy: &Progress, course: &Course) -> Result<Self, String> {
        legacy.validate()?;
        let lesson = course
            .lesson(LESSON_ID)
            .ok_or("Original lesson is missing from this course.")?;
        let mut result = Self::new(course);
        let original_activity = RELEASE_OBJECTIVES[legacy.current_step - 1];
        result.current = LearningSelection {
            lesson: LESSON_ID.into(),
            activity: if course.activity(LESSON_ID, original_activity).is_some() {
                original_activity.into()
            } else {
                lesson.activities[0].id.clone()
            },
        };
        for (id, old) in RELEASE_OBJECTIVES.iter().zip(&legacy.steps) {
            // Schema 1 only describes lesson v1/objective v1, even when importing
            // after an update. Keep that historical identity instead of granting
            // the same flag to the currently installed objective version.
            let progress = result
                .lessons
                .entry(LESSON_ID.into())
                .or_default()
                .entry(1)
                .or_default()
                .entry((*id).into())
                .or_default()
                .entry(1)
                .or_default();
            progress.prediction = old.predicted.clone();
            progress.checked = old.checked;
            progress.hints_revealed = usize::from(old.hint_revealed);
            progress.solution_revealed = old.solution_revealed;
            progress.legacy = true;
        }
        result.validate(course)?;
        Ok(result)
    }
    /// A compatibility view for the original seven-step controls, keyed by stable IDs.
    pub fn release_progress(&self, course: &Course) -> Progress {
        Progress {
            current_step: RELEASE_OBJECTIVES
                .iter()
                .position(|id| *id == self.current.activity)
                .map_or(1, |i| i + 1),
            steps: RELEASE_OBJECTIVES
                .iter()
                .map(|id| {
                    let p = self
                        .activity(course, LESSON_ID, id)
                        .cloned()
                        .unwrap_or_default();
                    StepProgress {
                        predicted: p.prediction,
                        checked: p.checked,
                        hint_revealed: p.hints_revealed > 0,
                        solution_revealed: p.solution_revealed,
                    }
                })
                .collect(),
            ..Progress::default()
        }
    }
    pub fn update_release(&mut self, legacy: &Progress, course: &Course) -> Result<(), String> {
        legacy.validate()?;
        for (id, old) in RELEASE_OBJECTIVES.iter().zip(&legacy.steps) {
            let current = self.activity_mut(course, LESSON_ID, id)?;
            current.prediction = old.predicted.clone();
            current.checked = old.checked;
            current.hints_revealed = current.hints_revealed.max(usize::from(old.hint_revealed));
            current.solution_revealed = old.solution_revealed;
        }
        Ok(())
    }
    pub fn import(text: &str, course: &Course) -> Result<Self, String> {
        if text.len() > 4 * 1024 * 1024 {
            return Err("Progress file exceeds 4 MiB.".into());
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("Invalid progress: {e}"))?;
        if value.get("schema").and_then(serde_json::Value::as_u64) == Some(1) {
            return Self::migrate_legacy(&Progress::import(text)?, course);
        }
        let result: Self =
            serde_json::from_value(value).map_err(|e| format!("Invalid progress: {e}"))?;
        result.validate(course)?;
        Ok(result)
    }
    pub fn validate(&self, course: &Course) -> Result<(), String> {
        if self.schema != PROGRESS_SCHEMA
            || self.course_version == 0
            || self.course_version > course.version
            || course
                .activity(&self.current.lesson, &self.current.activity)
                .is_none()
            || self.lessons.len() > 128
        {
            return Err("Incompatible progress schema, content version or current activity. Existing progress was retained.".into());
        }
        for (lesson_id, versions) in &self.lessons {
            let lesson = course
                .lesson(lesson_id)
                .ok_or("Progress refers to an unknown lesson.")?;
            if versions.len() > 32 {
                return Err("Too many historical lesson versions.".into());
            }
            for (version, activities) in versions {
                if *version == 0 || *version > lesson.version || activities.len() > 256 {
                    return Err(
                        "Unknown future lesson version or excessive activity history.".into(),
                    );
                }
                for (id, objective_versions) in activities {
                    if !valid_content_id(id) || objective_versions.len() > 32 {
                        return Err("Invalid objective history.".into());
                    }
                    let current = course.activity(lesson_id, id);
                    if *version == lesson.version && current.is_none() {
                        return Err("Unknown current objective.".into());
                    }
                    for (objective_version, progress) in objective_versions {
                        if *objective_version == 0
                            || (*version == lesson.version
                                && current.is_some_and(|a| *objective_version > a.version))
                            || progress.prediction.as_ref().is_some_and(|p| p.len() > 256)
                            || progress.reflection.as_ref().is_some_and(|p| p.len() > 8192)
                            || progress.hints_revealed > 128
                            || progress.actions.len() > 128
                            || progress.evidence.len() > 64
                            || progress.candidate_evidence.len() > 64
                            || progress.expected_failures.len() > 64
                            || progress.reviews.len() > 32
                            || progress.reviews.iter().any(|e| {
                                !matches!(
                                    e.event.as_str(),
                                    "prepared" | "rejected" | "refused" | "admitted"
                                ) || e.epoch.len() > 128
                                    || e.revision.len() > 32
                                    || e.source_id.len() > 256
                                    || e.source_sha256.len() != 64
                                    || e.formal_sha256.len() != 64
                                    || e.engine_revision.len() > 128
                                    || e.ticket.as_ref().is_some_and(|t| t.len() > 512)
                            })
                            || progress.actions.iter().any(|id| !valid_content_id(id))
                            || progress
                                .evidence
                                .iter()
                                .chain(&progress.candidate_evidence)
                                .any(|e| {
                                    e.run_id.len() > 512
                                        || e.engine_revision.len() > 128
                                        || e.epoch.len() > 512
                                        || e.revision.len() > 128
                                        || e.result_family.len() > 128
                                        || e.action.as_ref().is_some_and(|a| !valid_content_id(a))
                                        || e.outcome.as_ref().is_some_and(|o| o.len() > 256)
                                })
                            || progress.expected_failures.iter().any(|e| {
                                e.engine_revision.len() > 128
                                    || e.epoch.len() > 512
                                    || e.revision.len() > 128
                                    || e.stage.is_empty()
                                    || e.stage.len() > 128
                                    || e.error_class.is_empty()
                                    || e.error_class.len() > 128
                                    || e.action.as_ref().is_some_and(|a| !valid_content_id(a))
                            })
                        {
                            return Err("Invalid or incompatible objective evidence.".into());
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Activity, Lesson, RunProfile};
    fn course() -> Course {
        Course {
            schema: 2,
            version: 2,
            lessons: vec![Lesson {
                id: LESSON_ID.into(),
                version: 1,
                unit: "F5".into(),
                track: "foundations".into(),
                title: "Release".into(),
                summary: "Investigation".into(),
                prerequisites: vec![],
                legacy_release: true,
                activities: RELEASE_OBJECTIVES
                    .iter()
                    .map(|id| Activity {
                        id: (*id).into(),
                        version: 1,
                        title: "Step".into(),
                        objective: "Inspect a derivation".into(),
                        explanation: "Original input.".into(),
                        tasks: vec!["Run".into()],
                        hints: vec!["Inspect".into()],
                        solution: "Compare inputs.".into(),
                        setup: vec![],
                        profile: RunProfile::default(),
                        operation: None,
                        expected: None,
                        question: None,
                        actions: vec![],
                        references: vec![],
                        self_review: vec![],
                        local_lab: None,
                        retrieval: false,
                        cost_worksheet: false,
                        review_exercise: None,
                    })
                    .collect(),
            }],
        }
    }
    #[test]
    fn legacy_migration_preserves_all_seven_flags_without_inventing_runs() {
        let mut old = Progress {
            current_step: 5,
            ..Progress::default()
        };
        for (i, p) in old.steps.iter_mut().enumerate() {
            p.predicted = Some(if i % 2 == 0 { "TRUE" } else { "FALSE" }.into());
            p.checked = i < 4;
            p.hint_revealed = i % 2 == 0;
            p.solution_revealed = i == 6;
        }
        let catalog = course();
        let migrated =
            CourseProgress::import(&serde_json::to_string(&old).unwrap(), &catalog).unwrap();
        let restored = migrated.release_progress(&catalog);
        assert_eq!(restored.current_step, old.current_step);
        assert_eq!(restored.steps, old.steps);
        for id in RELEASE_OBJECTIVES {
            let p = migrated.activity(&catalog, LESSON_ID, id).unwrap();
            assert!(p.legacy);
            assert!(p.evidence.is_empty());
        }
    }
    #[test]
    fn historical_completion_cannot_complete_changed_objectives_or_accept_future_imports() {
        let mut catalog = course();
        let mut progress = CourseProgress::new(&catalog);
        progress
            .activity_mut(&catalog, LESSON_ID, RELEASE_OBJECTIVES[0])
            .unwrap()
            .checked = true;
        catalog.lessons[0].activities[0].version = 2;
        assert!(progress.validate(&catalog).is_ok());
        assert_eq!(progress.completed(&catalog), 0);
        assert!(progress.lessons[LESSON_ID][&1][RELEASE_OBJECTIVES[0]][&1].checked);
        progress.course_version = 3;
        assert!(
            CourseProgress::import(&serde_json::to_string(&progress).unwrap(), &catalog).is_err()
        );
        progress.course_version = 2;
        progress.current.activity = "unrecognized.objective".into();
        assert!(progress.validate(&catalog).is_err());
    }
    #[test]
    fn importing_original_flags_after_an_update_preserves_history_without_new_completion() {
        let mut catalog = course();
        catalog.lessons[0].version = 2;
        catalog.lessons[0].legacy_release = false;
        let mut old = Progress::default();
        old.steps[0].checked = true;
        old.steps[0].solution_revealed = true;
        let migrated = CourseProgress::migrate_legacy(&old, &catalog).unwrap();
        assert_eq!(migrated.completed(&catalog), 0);
        let historical = &migrated.lessons[LESSON_ID][&1][RELEASE_OBJECTIVES[0]][&1];
        assert!(historical.checked && historical.solution_revealed && historical.legacy);
        assert!(
            migrated
                .activity(&catalog, LESSON_ID, RELEASE_OBJECTIVES[0])
                .is_none()
        );
    }
}

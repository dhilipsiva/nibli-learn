//! Local project envelopes. Engine recovery validation happens in a detached worker candidate.
use crate::{
    Course, CourseProgress, ENGINE_REVISION, ProjectExport, RunProfile, RunSettings, SessionView,
    valid_content_id,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WORKSPACE_SCHEMA: u32 = 2;
pub const MAX_WORKSPACE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDocument {
    pub schema: u32,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub engine_revision: String,
    pub content_version: u32,
    pub lesson: Option<String>,
    pub activity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<ProjectOrigin>,
    pub session: SessionView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_draft: Option<crate::ReviewDraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_cost: Option<crate::ReviewCostInputs>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectOrigin {
    pub project_id: String,
    pub epoch: String,
    pub revision: String,
    pub engine_revision: String,
}
impl ProjectDocument {
    pub fn new(id: String, name: String, content_version: u32, session: SessionView) -> Self {
        Self {
            schema: WORKSPACE_SCHEMA,
            kind: "nibli-project".into(),
            id,
            name,
            engine_revision: ENGINE_REVISION.into(),
            content_version,
            lesson: None,
            activity: None,
            imported_from: None,
            review_draft: None,
            review_cost: None,
            session,
        }
    }
    pub fn validate(&self, course: &Course) -> Result<(), String> {
        if self.schema != WORKSPACE_SCHEMA
            || self.kind != "nibli-project"
            || !valid_content_id(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || self.content_version == 0
            || self.content_version > course.version
            || !matches!(self.engine_revision.as_str(), "legacy-unpinned")
                && self.engine_revision != ENGINE_REVISION
        {
            return Err("Incompatible project identity, schema, engine or content version.".into());
        }
        if self.session.epoch.is_empty()
            || self.session.epoch.len() > 128
            || self
                .session
                .revision
                .parse::<u64>()
                .ok()
                .is_none_or(|v| v.to_string() != self.session.revision)
            || self.session.settings.depth == 0
            || self.session.assertions.len() > 100_000
        {
            return Err("Invalid project session structure.".into());
        }
        if let Some(profile) = &self.session.profile {
            profile.validate()?;
        }
        if self.imported_from.as_ref().is_some_and(|origin| {
            !valid_content_id(&origin.project_id)
                || origin.epoch.is_empty()
                || origin.epoch.len() > 128
                || origin.engine_revision.is_empty()
                || origin.engine_revision.len() > 128
                || origin
                    .revision
                    .parse::<u64>()
                    .ok()
                    .is_none_or(|v| v.to_string() != origin.revision)
        }) {
            return Err("Invalid imported project provenance.".into());
        }
        if let Some(cost) = &self.review_cost {
            cost.validate()?;
        }
        if let Some(draft) = &self.review_draft {
            draft.validate_bounds()?;
        }
        match (&self.lesson, &self.activity) {
            (Some(lesson), Some(activity)) if course.activity(lesson, activity).is_some() => {}
            (None, None) => {}
            _ => return Err("Project lesson/activity association is invalid.".into()),
        }
        Ok(())
    }
    pub fn import(text: &str, course: &Course) -> Result<Self, String> {
        if text.len() > MAX_WORKSPACE_BYTES {
            return Err("Project exceeds 16 MiB.".into());
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("Invalid project: {e}"))?;
        let project = if value.get("schema").and_then(serde_json::Value::as_u64) == Some(1) {
            let old: ProjectExport = serde_json::from_value(value)
                .map_err(|e| format!("Invalid original project: {e}"))?;
            if old.kind != "nibli-project" {
                return Err("This file is not a project export.".into());
            }
            Self {
                schema: WORKSPACE_SCHEMA,
                kind: old.kind,
                id: "imported-project".into(),
                name: "Imported original project".into(),
                engine_revision: "legacy-unpinned".into(),
                content_version: 1,
                lesson: None,
                activity: None,
                imported_from: None,
                review_draft: None,
                review_cost: None,
                session: old.session,
            }
        } else {
            serde_json::from_value(value).map_err(|e| format!("Invalid project: {e}"))?
        };
        project.validate(course)?;
        Ok(project)
    }
    /// Call only after ValidateProject returned an engine-normalized, compatible image.
    pub fn accept_validated_session(&mut self, session: SessionView) {
        self.session = session;
        self.engine_revision = ENGINE_REVISION.into();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbortedAttemptRecord {
    pub id: String,
    pub claim: String,
    pub epoch: String,
    pub revision: String,
    pub settings: RunSettings,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalWorkspace {
    pub schema: u32,
    pub progress: CourseProgress,
    pub projects: BTreeMap<String, ProjectDocument>,
    pub active_project: Option<String>,
    pub theme: String,
    pub notation: String,
    #[serde(default)]
    pub aborted: Vec<AbortedAttemptRecord>,
    #[serde(default)]
    pub release_support_stage: usize,
    #[serde(default)]
    pub lab_reports: BTreeMap<String, crate::LabReport>,
}
impl LocalWorkspace {
    pub fn new(course: &Course) -> Self {
        Self {
            schema: WORKSPACE_SCHEMA,
            progress: CourseProgress::new(course),
            projects: BTreeMap::new(),
            active_project: None,
            theme: "dark".into(),
            notation: "both".into(),
            aborted: vec![],
            release_support_stage: 0,
            lab_reports: BTreeMap::new(),
        }
    }
    pub fn parse(text: &str, course: &Course) -> Result<Self, String> {
        if text.len() > MAX_WORKSPACE_BYTES {
            return Err("Local workspace exceeds 16 MiB.".into());
        }
        let workspace: Self =
            serde_json::from_str(text).map_err(|e| format!("Invalid local workspace: {e}"))?;
        workspace.validate(course)?;
        Ok(workspace)
    }
    pub fn validate(&self, course: &Course) -> Result<(), String> {
        if self.schema != WORKSPACE_SCHEMA
            || self.projects.len() > 64
            || !matches!(self.theme.as_str(), "dark" | "light")
            || !matches!(self.notation.as_str(), "plain" | "both" | "formal")
            || self.release_support_stage > 3
            || self.aborted.len() > 24
        {
            return Err("Invalid local workspace version or preferences.".into());
        }
        self.progress.validate(course)?;
        if self.lab_reports.len() > 16 {
            return Err("Too many local lab reports.".into());
        }
        for (id, report) in &self.lab_reports {
            if id != &report.lab_id {
                return Err("Local lab identity mismatch.".into());
            }
            report.validate()?;
        }
        for (id, project) in &self.projects {
            if id != &project.id {
                return Err("Project storage identity does not match its document.".into());
            }
            project.validate(course)?;
        }
        if self
            .active_project
            .as_ref()
            .is_some_and(|id| !self.projects.contains_key(id))
            || self.aborted.iter().any(|a| {
                a.status != "aborted"
                    || a.id.len() > 512
                    || a.epoch.len() > 128
                    || a.claim.len() > 1024 * 1024
                    || a.revision.len() > 128
            })
        {
            return Err("Invalid active project or aborted attempt record.".into());
        }
        Ok(())
    }
}

/// An old session's profile is read from its engine image, never invented from UI labels.
pub fn profile_from_session(session: &SessionView) -> Result<RunProfile, String> {
    if let Some(profile) = &session.profile {
        profile.validate()?;
        return Ok(profile.clone());
    }
    let profile = &session.recovery["engine"]["profile"];
    let read_bool = |field: &str| {
        profile[field]
            .as_bool()
            .ok_or_else(|| format!("Missing recovered profile {field}."))
    };
    let result = RunProfile {
        id: "teaching".into(),
        version: 1,
        strict: read_bool("strict")?,
        existential_import: read_bool("existential_import")?,
        materialization: read_bool("materialization")?,
        max_chain_depth: profile["max_chain_depth"]
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("Missing recovered depth.")?,
    };
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn course() -> Course {
        Course::parse(include_str!("../../../content/course.json")).unwrap()
    }
    fn session() -> SessionView {
        SessionView {
            epoch: "exported-epoch".into(),
            revision: "3".into(),
            settings: RunSettings::default(),
            profile: None,
            engine_revision: None,
            assertions: vec![],
            recovery: json!({"engine":{"profile":{
                "strict":true,"existential_import":false,
                "materialization":true,"max_chain_depth":17
            }}}),
        }
    }
    #[test]
    fn original_project_import_is_untrusted_until_engine_validation() {
        let catalog = course();
        let original = json!({"schema":1,"kind":"nibli-project","session":session()});
        let mut imported = ProjectDocument::import(&original.to_string(), &catalog).unwrap();
        assert_eq!(imported.engine_revision, "legacy-unpinned");
        assert_eq!(imported.session.epoch, "exported-epoch");
        let profile = profile_from_session(&imported.session).unwrap();
        assert!(profile.strict && profile.materialization);
        assert_eq!(profile.max_chain_depth, 17);
        // Parsing retains the image without claiming source replay was validated.
        assert_eq!(imported.session.recovery, original["session"]["recovery"]);
        let mut normalized = session();
        normalized.epoch = "new-local-epoch".into();
        imported.accept_validated_session(normalized);
        assert_eq!(imported.engine_revision, ENGINE_REVISION);
        assert_eq!(imported.session.epoch, "new-local-epoch");
    }
    #[test]
    fn incompatible_project_and_workspace_inputs_cannot_be_applied() {
        let catalog = course();
        let project = ProjectDocument::new("mine".into(), "My project".into(), 2, session());
        for (field, value) in [
            ("schema", json!(3)),
            ("engine_revision", json!("unknown-engine")),
            ("content_version", json!(catalog.version + 1)),
            ("kind", json!("proof-record")),
            ("lesson", json!("unknown.lesson")),
        ] {
            let mut input = serde_json::to_value(&project).unwrap();
            input[field] = value;
            assert!(
                ProjectDocument::import(&input.to_string(), &catalog).is_err(),
                "{field}"
            );
        }
        let mut workspace = LocalWorkspace::new(&catalog);
        workspace.projects.insert("mine".into(), project.clone());
        workspace.active_project = Some("missing".into());
        assert!(workspace.validate(&catalog).is_err());
        workspace.active_project = Some("mine".into());
        assert!(workspace.validate(&catalog).is_ok());
        workspace.projects.insert("alias".into(), project);
        assert!(workspace.validate(&catalog).is_err());
    }
    #[test]
    fn missing_legacy_profile_is_an_error_instead_of_default_flags() {
        let mut image = session();
        image.recovery["engine"]["profile"]
            .as_object_mut()
            .unwrap()
            .remove("strict");
        assert!(profile_from_session(&image).is_err());
    }
}

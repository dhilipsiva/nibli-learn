use crate::state::*;
use dioxus::dioxus_core::spawn_forever;
use dioxus::prelude::*;
use nibli_learn_client::{WorkerClient, platform};
use nibli_learn_model::*;
use serde_json::Value;

#[derive(Clone)]
pub enum ImportPreview {
    Progress(CourseProgress),
    Project(Box<ProjectDocument>),
    Workspace(LocalWorkspace),
    Archive(Value),
}

pub fn capture(mut state: Signal<AppState>) {
    let mut s = state.write();
    let progress = s.progress.clone();
    let course = s.learning.course.clone();
    let _ = s
        .learning
        .workspace
        .progress
        .update_release(&progress, &course);
    s.learning.workspace.theme = s.theme.clone();
    s.learning.workspace.notation = s.notation.clone();
    s.learning.workspace.release_support_stage = s.support_stage;
    s.learning.workspace.aborted = s
        .aborted
        .iter()
        .take(24)
        .map(|a| AbortedAttemptRecord {
            id: a.id.clone(),
            claim: a.claim.clone(),
            epoch: a.epoch.clone(),
            revision: a.revision.clone(),
            settings: a.settings.clone(),
            status: a.status.clone(),
        })
        .collect();
    if let Some(session) = s.session.clone() {
        let id = s
            .learning
            .workspace
            .active_project
            .clone()
            .unwrap_or_else(|| "original-release".into());
        let mut p = s
            .learning
            .workspace
            .projects
            .get(&id)
            .cloned()
            .unwrap_or_else(|| {
                ProjectDocument::new(
                    id.clone(),
                    if s.screen == Screen::Sandbox {
                        "Sandbox project".into()
                    } else {
                        s.learning.lesson_title()
                    },
                    course.version,
                    session.clone(),
                )
            });
        p.session = session;
        p.review_draft = Some(s.review.draft.clone());
        p.review_cost = Some(s.learning.review_cost.clone());
        p.engine_revision = ENGINE_REVISION.into();
        if s.screen == Screen::Lesson
            || (p.lesson.is_none() && id == "original-release" && s.learning.is_legacy())
        {
            let c = &s.learning.workspace.progress.current;
            p.lesson = Some(c.lesson.clone());
            p.activity = Some(c.activity.clone());
        }
        s.learning.workspace.projects.insert(id.clone(), p);
        s.learning.workspace.active_project = Some(id);
    }
}
fn load_into(
    mut state: Signal<AppState>,
    envelope: platform::WorkspaceEnvelope,
) -> Result<(), String> {
    // Preserve the CAS receipt before semantic validation. Invalid content remains
    // opaque in storage until an explicit validated replacement creates its backup.
    {
        let mut s = state.write();
        s.learning.storage_revision = Some(envelope.revision.clone());
        s.learning.can_undo = envelope.can_undo;
    }
    let parsed = {
        let s = state.peek();
        LocalWorkspace::parse(&envelope.data.to_string(), &s.learning.course)
    };
    let workspace = match parsed {
        Ok(workspace) => workspace,
        Err(message) => {
            let mut s = state.write();
            s.storage_locked = true;
            s.storage_error = format!(
                "Saved workspace was retained: {message} Export the raw data or explicitly apply a validated replacement project/workspace."
            );
            return Err(s.storage_error.clone());
        }
    };
    let mut s = state.write();
    s.theme = workspace.theme.clone();
    s.notation = workspace.notation.clone();
    s.progress = workspace.progress.release_progress(&s.learning.course);
    s.support_stage = workspace.release_support_stage;
    s.aborted = workspace
        .aborted
        .iter()
        .map(|a| AbortedAttempt {
            id: a.id.clone(),
            claim: a.claim.clone(),
            epoch: a.epoch.clone(),
            revision: a.revision.clone(),
            settings: a.settings.clone(),
            status: a.status.clone(),
        })
        .collect();
    s.session = workspace
        .active_project
        .as_ref()
        .and_then(|id| workspace.projects.get(id))
        .map(|p| p.session.clone());
    s.learning.workspace = workspace;
    s.learning.storage_revision = Some(envelope.revision);
    s.learning.can_undo = envelope.can_undo;
    s.storage_locked = false;
    s.storage_error.clear();
    s.learning.cross_tab = false;
    s.learning.save_dirty = false;
    Ok(())
}

pub fn subscribe(mut state: Signal<AppState>) -> platform::WorkspaceSubscription {
    platform::subscribe_workspace_changes(move |event| {
        if event["own"] == true {
            return;
        }
        if matches!(
            event["kind"].as_str(),
            Some("workspace_changed" | "storage_version_changed")
        ) {
            let mut s = state.write();
            s.learning.cross_tab = true;
            s.storage_locked = true;
            s.storage_error="Another tab changed the local workspace. This tab’s unsaved work is retained. Export a copy or explicitly reload the saved workspace.".into();
        }
    })
}
pub fn boot(mut state: Signal<AppState>) {
    spawn_forever(async move {
        load_paired_content(state).await;
        match platform::load_workspace().await {
            Ok(Some(envelope)) => {
                if let Err(message) = load_into(state, envelope) {
                    let mut s = state.write();
                    s.storage_locked = true;
                    s.storage_error = message;
                }
            }
            Ok(None) => {}
            Err(error) => {
                state.write().storage_error = format!(
                    "Local storage is unavailable: {} Work remains exportable in this tab.",
                    error.message
                );
            }
        }
        state.write().learning.loaded = true;
        if state.peek().session.is_some()
            && state.peek().learning.workspace.active_project.is_none()
        {
            capture(state);
        }
        if let Some((lesson, activity)) = crate::curriculum::selection_from_hash(&current_hash()) {
            if state
                .peek()
                .learning
                .course
                .activity(&lesson, &activity)
                .is_some()
            {
                let id =
                    project_id_for_selection(&state.peek().learning.workspace, &lesson, &activity);
                let mut s = state.write();
                s.learning.workspace.progress.current = LearningSelection { lesson, activity };
                s.learning.workspace.active_project = Some(id.clone());
                s.session = s
                    .learning
                    .workspace
                    .projects
                    .get(&id)
                    .map(|p| p.session.clone());
            }
        } else if current_hash().contains("/step/") {
            let step = current_hash()
                .split("/step/")
                .nth(1)
                .and_then(|v| v.split('?').next())
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|n| (1..=7).contains(n))
                .unwrap_or(1);
            let id = project_id_for_selection(
                &state.peek().learning.workspace,
                LESSON_ID,
                RELEASE_OBJECTIVES[step - 1],
            );
            let mut s = state.write();
            s.session = s
                .learning
                .workspace
                .projects
                .get(&id)
                .filter(|p| {
                    p.lesson.as_deref() == Some(LESSON_ID)
                        && p.activity.as_deref() == Some(RELEASE_OBJECTIVES[step - 1])
                })
                .map(|p| p.session.clone());
            s.learning.workspace.progress.current = LearningSelection {
                lesson: LESSON_ID.into(),
                activity: RELEASE_OBJECTIVES[step - 1].into(),
            };
            s.learning.workspace.active_project = Some(id);
            s.progress.current_step = step;
        }
        start_worker(state);
    });
}
fn start_worker(mut state: Signal<AppState>) {
    let client = match WorkerClient::new() {
        Ok(client) => client,
        Err(message) => {
            let mut s = state.write();
            s.busy = false;
            s.operation = "error".into();
            s.storage_error = message;
            return;
        }
    };
    let command = {
        let mut s = state.write();
        s.client = Some(client);
        s.busy = false;
        if let Some(session) = s.session.clone() {
            Command::Restore { session }
        } else if s.learning.is_legacy() {
            s.learning.workspace.active_project = Some("original-release".into());
            Command::Initialize {
                epoch: epoch(),
                statements: lesson_setup(s.step()),
                settings: RunSettings::default(),
            }
        } else {
            let a = s.learning.activity().unwrap().clone();
            let c = &s.learning.workspace.progress.current;
            let id = format!("{}-{}", c.lesson, c.activity);
            s.learning.workspace.active_project = Some(id);
            Command::InitializeProfile {
                epoch: epoch(),
                statements: a.setup,
                profile: a.profile,
                recording: RecordingLevel::Full,
            }
        }
    };
    send(state, command, Purpose::Initialize);
}

pub fn persist(mut state: Signal<AppState>, backup: bool) {
    capture(state);
    {
        let mut s = state.write();
        s.learning.save_dirty = true;
        s.learning.backup_pending |= backup;
        if s.storage_locked || !s.learning.loaded || s.learning.save_busy {
            return;
        }
        s.learning.save_busy = true;
    }
    spawn_forever(async move {
        loop {
            let (expected, data, backup) = {
                let mut s = state.write();
                if s.storage_locked || !s.learning.save_dirty {
                    s.learning.save_busy = false;
                    return;
                }
                if let Err(message) = s.learning.workspace.validate(&s.learning.course) {
                    s.storage_error = format!("Not saved: {message} Export your current work.");
                    s.learning.save_busy = false;
                    return;
                }
                s.learning.save_dirty = false;
                let backup = s.learning.backup_pending;
                s.learning.backup_pending = false;
                (
                    s.learning.storage_revision.clone(),
                    serde_json::to_value(&s.learning.workspace).unwrap_or(Value::Null),
                    backup,
                )
            };
            match platform::save_workspace(expected, data, backup).await {
                Ok(envelope) => {
                    let mut s = state.write();
                    s.learning.storage_revision = Some(envelope.revision);
                    s.learning.can_undo = envelope.can_undo;
                    if !s.learning.cross_tab {
                        s.storage_error.clear();
                    }
                }
                Err(error) => {
                    let mut s = state.write();
                    s.learning.save_busy = false;
                    s.learning.save_dirty = true;
                    s.learning.backup_pending |= backup;
                    if matches!(error.kind.as_str(), "conflict" | "version_conflict") {
                        s.storage_locked = true;
                        s.learning.cross_tab = true;
                    }
                    s.storage_error = format!(
                        "Not saved ({}): {} Your work remains exportable in this tab.",
                        error.kind, error.message
                    );
                    return;
                }
            }
        }
    });
}
pub fn reload(mut state: Signal<AppState>) {
    if state.peek().busy || state.peek().learning.save_busy {
        return;
    }
    spawn_forever(async move {
        match platform::load_workspace().await {
            Ok(Some(envelope)) => match load_into(state, envelope) {
                Ok(()) => {
                    state.write().notice =
                        "Reloaded the saved workspace by explicit request.".into();
                    start_worker(state);
                }
                Err(message) => state.write().storage_error = message,
            },
            Ok(None) => {
                state.write().storage_error =
                    "No saved workspace was found. Current work was retained.".into()
            }
            Err(error) => state.write().storage_error = error.message,
        }
    });
}
fn undo(mut state: Signal<AppState>) {
    let Some(revision) = state.peek().learning.storage_revision.clone() else {
        return;
    };
    if state.peek().busy || state.peek().learning.save_busy {
        return;
    }
    spawn_forever(async move {
        match platform::undo_workspace(revision).await {
            Ok(envelope) => match load_into(state, envelope) {
                Ok(()) => {
                    state.write().notice =
                        "Restored the saved backup. A fresh engine validation is running.".into();
                    start_worker(state);
                }
                Err(e) => state.write().storage_error = e,
            },
            Err(e) => state.write().storage_error = e.message,
        }
    });
}
fn export_workspace(state: Signal<AppState>) {
    capture(state);
    download(
        "nibli-local-workspace.json",
        serde_json::to_string_pretty(&state.peek().learning.workspace).unwrap_or_default(),
    );
}
fn export_raw(mut state: Signal<AppState>) {
    spawn_forever(async move {
        match platform::raw_existing_data().await {
            Ok(value) => download(
                "nibli-retained-storage.json",
                serde_json::to_string_pretty(&value).unwrap_or_default(),
            ),
            Err(e) => state.write().export_message = e.message,
        }
    });
}
fn new_project(mut state: Signal<AppState>) {
    if state.peek().busy {
        return;
    }
    capture(state);
    if state.peek().learning.workspace.projects.len() >= 64 {
        state.write().notice="This workspace already has 64 projects. Export an existing project before replacing your local workspace.".into();
        return;
    }
    let id = format!("project-{}", epoch());
    {
        let mut s = state.write();
        s.learning.workspace.active_project = Some(id);
        s.screen = Screen::Sandbox;
        s.sandbox_claim = "ready(Cart7, Audit).".into();
        s.selected_run = None;
        s.learning.selected_operation = None;
    }
    send(
        state,
        Command::InitializeProfile {
            epoch: epoch(),
            statements: vec![],
            profile: RunProfile::default(),
            recording: RecordingLevel::Full,
        },
        Purpose::Initialize,
    );
    update_route(state);
}
fn open_project(mut state: Signal<AppState>, id: &str) {
    if state.peek().busy {
        return;
    }
    capture(state);
    let Some(project) = state.peek().learning.workspace.projects.get(id).cloned() else {
        return;
    };
    {
        let mut s = state.write();
        s.learning.workspace.active_project = Some(id.into());
        if let (Some(lesson), Some(activity)) = (&project.lesson, &project.activity) {
            s.learning.workspace.progress.current = LearningSelection {
                lesson: lesson.clone(),
                activity: activity.clone(),
            };
            s.screen = Screen::Lesson;
            s.progress = s
                .learning
                .workspace
                .progress
                .release_progress(&s.learning.course);
        } else {
            s.screen = Screen::Sandbox;
        }
        s.selected_run = None;
        s.learning.selected_operation = None;
        s.learning.action_cursor = 0;
        s.learning.feedback="Opened a saved project. Authored action preconditions may require resetting the activity.".into();
    }
    send(
        state,
        Command::Restore {
            session: project.session,
        },
        Purpose::Initialize,
    );
    update_route(state);
}
fn preview_import(mut state: Signal<AppState>) {
    let (kind, text, course) = {
        let mut s = state.write();
        s.import_error.clear();
        s.learning.preview = None;
        (
            s.learning.import_kind.clone(),
            s.import_text.clone(),
            s.learning.course.clone(),
        )
    };
    if kind == "progress" {
        match CourseProgress::import(&text, &course) {
            Ok(progress) => {
                state.write().learning.preview = Some(ImportPreview::Progress(progress))
            }
            Err(e) => state.write().import_error = e,
        }
        return;
    }
    if kind == "archive" {
        if text.len() > 64 * 1024 * 1024 {
            state.write().import_error = "Archive exceeds 64 MiB.".into();
            return;
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(value) => {
                let valid = if let Some(run) = value.get("run") {
                    value["schema"].as_u64() == Some(1)
                        && value["kind"] == "proof-record"
                        && serde_json::from_value::<RunRecord>(run.clone()).is_ok()
                } else if value.get("schema").is_some() {
                    serde_json::from_value::<OperationRun>(value.clone())
                        .is_ok_and(|run| run.schema == OPERATION_SCHEMA)
                } else {
                    serde_json::from_value::<RunRecord>(value.clone()).is_ok()
                };
                if valid {
                    state.write().learning.preview = Some(ImportPreview::Archive(value));
                } else {
                    state.write().import_error="This is not a supported run/proof archive. Existing evidence was retained.".into();
                }
            }
            Err(e) => state.write().import_error = e.to_string(),
        }
        return;
    }
    if kind == "workspace" {
        let mut workspace = match LocalWorkspace::parse(&text, &course) {
            Ok(w) => w,
            Err(e) => {
                state.write().import_error = e;
                return;
            }
        };
        let Some(client) = state.peek().client.clone() else {
            return;
        };
        if state.peek().busy {
            return;
        }
        state.write().busy = true;
        spawn_forever(async move {
            let mut imported = std::collections::BTreeMap::new();
            let previous_active = workspace.active_project.clone();
            for (old_id, mut project) in std::mem::take(&mut workspace.projects) {
                renew_import_identity(&mut project);
                if previous_active.as_deref() == Some(&old_id) {
                    workspace.active_project = Some(project.id.clone());
                }
                match client
                    .request(Command::ValidateProject {
                        session: project.session.clone(),
                    })
                    .await
                {
                    Ok(reply) => {
                        if let Some(session) = reply.validated_session {
                            project.accept_validated_session(session);
                        } else {
                            state.write().import_error =
                                "Workspace validation returned no normalized project.".into();
                            state.write().busy = false;
                            return;
                        }
                    }
                    Err(error) => {
                        state.write().import_error = format!(
                            "Workspace retained: project {} failed detached validation: {}",
                            project.name, error.message
                        );
                        state.write().busy = false;
                        return;
                    }
                }
                imported.insert(project.id.clone(), project);
            }
            workspace.projects = imported;
            state.write().busy = false;
            state.write().learning.preview = Some(ImportPreview::Workspace(workspace));
        });
        return;
    }
    let mut project = match ProjectDocument::import(&text, &course) {
        Ok(p) => p,
        Err(e) => {
            state.write().import_error = e;
            return;
        }
    };
    renew_import_identity(&mut project);
    let Some(client) = state.peek().client.clone() else {
        return;
    };
    if state.peek().busy {
        return;
    }
    state.write().busy = true;
    spawn_forever(async move {
        let result = client
            .request(Command::ValidateProject {
                session: project.session.clone(),
            })
            .await;
        state.write().busy = false;
        match result {
            Ok(reply) => {
                if let Some(session) = reply.validated_session {
                    let mut project = project;
                    project.accept_validated_session(session);
                    state.write().learning.preview =
                        Some(ImportPreview::Project(Box::new(project)));
                } else {
                    state.write().import_error =
                        "Validation returned no normalized project. The main project was retained."
                            .into();
                }
            }
            Err(error) => {
                state.write().import_error =
                    format!("Project refused in detached validation: {}", error.message)
            }
        }
    });
}
fn apply_import(mut state: Signal<AppState>) {
    let Some(preview) = state.peek().learning.preview.clone() else {
        return;
    };
    if state.peek().busy {
        return;
    }
    if !matches!(preview, ImportPreview::Archive(_)) {
        if state.peek().learning.cross_tab {
            state.write().import_error="Another tab changed the saved workspace. This import was not applied. Export this tab’s copy or explicitly reload the latest saved workspace first.".into();
            return;
        }
        if state.peek().storage_locked {
            if matches!(
                preview,
                ImportPreview::Workspace(_) | ImportPreview::Project(_)
            ) {
                let mut s = state.write();
                s.storage_locked = false;
                s.storage_error.clear();
                s.learning.backup_pending = true;
            } else {
                state.write().import_error="Recover unreadable saved data with a validated whole workspace or project. A progress-only import cannot replace unknown source knowledge.".into();
                return;
            }
        }
    }
    match preview {
        ImportPreview::Progress(progress) => {
            let mut s = state.write();
            s.learning.workspace.progress = progress;
            s.progress = s
                .learning
                .workspace
                .progress
                .release_progress(&s.learning.course);
            s.learning.preview = None;
            s.notice =
                "Imported validated progress. Earlier source/run files remain separate evidence."
                    .into();
            drop(s);
            persist(state, true);
        }
        ImportPreview::Workspace(workspace) => {
            let mut s = state.write();
            s.support_stage = workspace.release_support_stage;
            s.aborted = workspace
                .aborted
                .iter()
                .map(|a| AbortedAttempt {
                    id: a.id.clone(),
                    claim: a.claim.clone(),
                    epoch: a.epoch.clone(),
                    revision: a.revision.clone(),
                    settings: a.settings.clone(),
                    status: a.status.clone(),
                })
                .collect();
            s.learning.workspace = workspace;
            s.progress = s
                .learning
                .workspace
                .progress
                .release_progress(&s.learning.course);
            s.theme = s.learning.workspace.theme.clone();
            s.notation = s.learning.workspace.notation.clone();
            s.session = s
                .learning
                .workspace
                .active_project
                .as_ref()
                .and_then(|id| s.learning.workspace.projects.get(id))
                .map(|p| p.session.clone());
            s.learning.preview = None;
            s.learning.backup_pending = true;
            s.screen = Screen::Course;
            drop(s);
            persist(state, true);
            start_worker(state);
            update_route(state);
        }
        ImportPreview::Archive(value) => {
            let mut s = state.write();
            s.learning.archive = Some(value);
            s.learning.preview = None;
        }
        ImportPreview::Project(project) => {
            let project = *project;
            capture(state);
            let id = project.id.clone();
            {
                let mut s = state.write();
                s.learning
                    .workspace
                    .projects
                    .insert(id.clone(), project.clone());
                s.learning.workspace.active_project = Some(id);
                s.learning.preview = None;
                s.learning.backup_pending = true;
                if let (Some(lesson), Some(activity)) = (&project.lesson, &project.activity) {
                    s.learning.workspace.progress.current = LearningSelection {
                        lesson: lesson.clone(),
                        activity: activity.clone(),
                    };
                    s.screen = Screen::Lesson;
                    s.progress = s
                        .learning
                        .workspace
                        .progress
                        .release_progress(&s.learning.course);
                } else {
                    s.screen = Screen::Sandbox;
                }
            }
            send(
                state,
                Command::Restore {
                    session: project.session,
                },
                Purpose::Initialize,
            );
            update_route(state);
        }
    }
}

#[component]
pub fn DataPage() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let projects = s
        .learning
        .workspace
        .projects
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let progress_count = s.learning.workspace.progress.completed(&s.learning.course);
    let total: usize = s
        .learning
        .course
        .lessons
        .iter()
        .map(|l| l.activities.len())
        .sum();
    rsx! {main{id:"main-content",tabindex:"-1",class:"data-page",div{class:"page-heading",span{class:"eyebrow","Local data · no account"}h1{"Your projects, your evidence."}p{"Progress and source projects stay on this device. Export a copy before clearing browser storage or moving to another browser."}}
        if !s.storage_error.is_empty(){div{class:"notice error",role:"alert","{s.storage_error}"}}if s.learning.cross_tab{div{class:"notice",p{"Another tab has newer saved work. Current unsaved changes have not been overwritten."}div{class:"button-row",button{class:"button",onclick:move |_|export_workspace(state),"Export this tab’s copy"}button{class:"button primary",disabled:s.busy||s.learning.save_busy,onclick:move |_|reload(state),"Reload saved workspace"}}}}
        div{class:"data-grid",section{class:"data-card",h2{"Learning progress"}p{"{progress_count} of {total} current activities checked. Earlier objective versions remain historical."}div{class:"button-row",button{class:"button",onclick:move |_|download("nibli-course-progress.json",serde_json::to_string_pretty(&state.peek().learning.workspace.progress).unwrap_or_default()),"Export progress"}button{class:"button",onclick:move |_|export_workspace(state),"Export all local work"}}p{class:"small muted","The original schema-1 local storage key is retained during migration. New saves use the versioned browser database."}}
        section{class:"data-card",h2{"Backup and recovery"}p{"Explicit imports create a saved backup. Failed validation preserves existing data."}div{class:"button-row",button{class:"button",disabled:!s.learning.can_undo||s.busy||s.learning.save_busy,onclick:move |_|undo(state),"Undo last replacement"}button{class:"button",onclick:move |_|export_raw(state),"Export retained raw data"}button{class:"button",disabled:s.busy,onclick:move |_|cancel(state),"Restore acknowledged project"}}if !s.export_message.is_empty(){p{"{s.export_message}"}}}}
        section{class:"data-card",div{class:"section-heading",h2{"Local projects ({projects.len()})"}button{class:"button primary interactive-only",disabled:s.busy,onclick:move |_|new_project(state),"New sandbox project"}}for project in projects{{let open_id=project.id.clone();let export=project.clone();let rename_id=project.id.clone();rsx!{article{key:"{project.id}",class:"project-row",div{label{"Project name" input{value:"{project.name}",maxlength:"256",oninput:move|e|{if let Some(p)=state.write().learning.workspace.projects.get_mut(&rename_id){p.name=e.value();}persist(state,false);}}}p{class:"small muted","Revision {project.session.revision} · {project.session.assertions.iter().filter(|a|a.active).count()} accepted records"}}div{class:"button-row",button{class:"button",disabled:s.busy,onclick:move |_|open_project(state,&open_id),"Open"}button{class:"button",onclick:move |_|download("nibli-project.json",serde_json::to_string_pretty(&export).unwrap_or_default()),"Export project"}}}}}}}
        section{class:"data-card",h2{"Import with a preview"}p{"Project validation happens in a detached engine candidate. Applying it is a separate explicit action. Run archives remain read-only and never grant current progress."}label{"Import type" select{value:"{s.learning.import_kind}",onchange:move|e|{state.write().learning.import_kind=e.value();state.write().learning.preview=None;},option{value:"progress","Learning progress (schema 1 or 2)"}option{value:"project","Source project"}option{value:"workspace","All local work / workspace"}option{value:"archive","Read-only run / proof archive"}}}label{"Exported JSON" textarea{rows:"7",value:"{s.import_text}",oninput:move|e|{state.write().import_text=e.value();state.write().learning.preview=None;},placeholder:"Paste an exported file’s JSON here"}}button{class:"button",disabled:s.busy,onclick:move |_|preview_import(state),"Validate and preview"}if !s.import_error.is_empty(){p{class:"notice error",role:"alert","{s.import_error}"}}
            if let Some(preview)=s.learning.preview.clone(){div{class:"import-preview",h3{"Validated preview"}match preview{ImportPreview::Progress(p)=>rsx!{p{"Progress schema {p.schema}, course version {p.course_version}. {p.completed(&s.learning.course)} current activities checked."}},ImportPreview::Project(p)=>rsx!{p{"{p.name}: {p.session.assertions.iter().filter(|a|a.active).count()} accepted source records."}details{summary{"Validated project and profile"}pre{"{serde_json::to_string_pretty(&p).unwrap_or_default()}"}}},ImportPreview::Workspace(w)=>rsx!{p{"Workspace schema {w.schema}: {w.projects.len()} independently validated projects and saved learning progress."}},ImportPreview::Archive(_)=>rsx!{p{"Read-only archive. It cannot become a current run, mutate knowledge or complete objectives."}}}div{class:"button-row",button{class:"button",onclick:move |_|state.write().learning.preview=None,"Cancel import"}button{class:"button primary",disabled:s.busy||s.learning.save_busy,onclick:move |_|apply_import(state),"Apply this validated import"}}}}
        }
        if let Some(archive)=&s.learning.archive{section{class:"data-card archived-run",h2{"Read-only archived evidence"}p{"Imported, unverified evidence: this read-only record is not a current project result. Its original identifiers and inputs remain unchanged."}button{class:"button",onclick:move |_|state.write().learning.archive=None,"Close archive"}pre{"{serde_json::to_string_pretty(archive).unwrap_or_default()}"}}}
    }}
}

fn renew_import_identity(project: &mut ProjectDocument) {
    project.imported_from = Some(ProjectOrigin {
        project_id: project.id.clone(),
        epoch: project.session.epoch.clone(),
        revision: project.session.revision.clone(),
        engine_revision: project.engine_revision.clone(),
    });
    project.id = format!("import-{}", epoch());
    project.session.epoch = epoch();
    project.session.revision = "0".into();
}
async fn load_paired_content(mut state: Signal<AppState>) {
    let status = match platform::offline_status().await {
        Ok(v) => v,
        Err(_) => return,
    };
    let active = &status["active"];
    let id = if active["runtime_id"].as_str() == Some(RUNTIME_ID) {
        active["installation_id"].as_str().map(str::to_owned)
    } else {
        status["installations"]
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|i| i["runtime_id"].as_str() == Some(RUNTIME_ID) && i["complete"] == true)
            })
            .and_then(|i| i["id"].as_str())
            .map(str::to_owned)
    };
    if let Some(id) = id {
        let result = async {
            let content = platform::installed_content(id)
                .await
                .map_err(|e| e.message)?;
            let course = Course::parse(&content["course"].to_string())?;
            let references = ReferenceLibrary::parse(&content["references"].to_string(), &course)?;
            let labs = if course.schema >= 3 {
                let labs = LabLibrary::parse(&content["labs"].to_string())?;
                labs.validate_links(&course, &references)?;
                Some(labs)
            } else {
                None
            };
            Ok::<_, String>((course, references, labs))
        }
        .await;
        match result {
            Ok((course, references, labs)) => {
                let mut s = state.write();
                let mut workspace = LocalWorkspace::new(&course);
                if s.session.is_some()
                    && let Ok(progress) = CourseProgress::migrate_legacy(&s.progress, &course)
                {
                    workspace.progress = progress;
                }
                s.learning.workspace = workspace;
                s.learning.course = course;
                s.learning.references = references;
                if let Some(labs) = labs {
                    s.learning.labs = labs;
                }
            }
            Err(message) => {
                state.write().notice = format!(
                    "Verified installed content could not be loaded: {message} This page is using its bundled original content."
                )
            }
        }
    } else if !active.is_null() && active["runtime_id"].as_str() != Some(RUNTIME_ID) {
        state.write().notice="A different offline runtime is active. This page retains its bundled matching content; reload from Offline packs to use the active version.".into();
    }
}

/// A route selects learning content, not a replacement for the learner's project.
pub fn project_id_for_selection(
    workspace: &LocalWorkspace,
    lesson: &str,
    activity: &str,
) -> String {
    workspace
        .active_project
        .as_ref()
        .filter(|id| {
            workspace.projects.get(*id).is_some_and(|p| {
                p.lesson.as_deref() == Some(lesson) && p.activity.as_deref() == Some(activity)
            })
        })
        .cloned()
        .unwrap_or_else(|| {
            if lesson == LESSON_ID {
                "original-release".into()
            } else {
                format!("{lesson}-{activity}")
            }
        })
}

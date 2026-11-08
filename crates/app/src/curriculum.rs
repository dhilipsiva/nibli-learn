use crate::state::*;
use dioxus::prelude::*;
use nibli_learn_model::*;
use serde_json::Value;

pub struct LearningState {
    pub course: Course,
    pub references: ReferenceLibrary,
    pub labs: LabLibrary,
    pub lab_input: String,
    pub lab_feedback: String,
    pub retrieval_query: String,
    pub retrieval_scope: String,
    pub retrieval_withdrawn: bool,
    pub review_cost: ReviewCostInputs,
    pub review_cost_fields: std::collections::BTreeMap<&'static str, String>,
    pub review_cost_feedback: String,
    pub workspace: LocalWorkspace,
    pub storage_revision: Option<String>,
    pub save_busy: bool,
    pub save_dirty: bool,
    pub backup_pending: bool,
    pub can_undo: bool,
    pub cross_tab: bool,
    pub loaded: bool,
    pub search: String,
    pub track: String,
    pub reference_search: String,
    pub compile_source: String,
    pub operation_runs: Vec<OperationRun>,
    pub selected_operation: Option<usize>,
    pub candidate: Option<HypotheticalResult>,
    pub apply_dialog: bool,
    pub action_cursor: usize,
    pub baseline_done: bool,
    pub feedback: String,
    pub failure: Option<OperationResult>,
    pub import_kind: String,
    pub preview: Option<crate::local::ImportPreview>,
    pub archive: Option<Value>,
}
impl LearningState {
    pub fn new(legacy: &Progress, has_legacy: bool) -> Self {
        let course = Course::parse(include_str!("../../../content/course.json"))
            .expect("validated bundled course");
        let references =
            ReferenceLibrary::parse(include_str!("../../../content/references.json"), &course)
                .expect("validated bundled references");
        let mut workspace = LocalWorkspace::new(&course);
        if has_legacy {
            workspace.progress =
                CourseProgress::migrate_legacy(legacy, &course).expect("validated legacy progress");
        }
        Self {
            course,
            references,
            labs: LabLibrary::parse(include_str!("../../../content/labs.json"))
                .expect("validated local lab catalog"),
            lab_input: String::new(),
            lab_feedback: String::new(),
            retrieval_query: "review approved".into(),
            retrieval_scope: "CaseA".into(),
            retrieval_withdrawn: false,
            review_cost: ReviewCostInputs::default(),
            review_cost_fields: std::collections::BTreeMap::new(),
            review_cost_feedback: String::new(),
            workspace,
            storage_revision: None,
            save_busy: false,
            save_dirty: false,
            backup_pending: false,
            can_undo: false,
            cross_tab: false,
            loaded: false,
            search: String::new(),
            track: "all".into(),
            reference_search: String::new(),
            compile_source: "approves(Reviewer, Cart7).".into(),
            operation_runs: vec![],
            selected_operation: None,
            candidate: None,
            apply_dialog: false,
            action_cursor: 0,
            baseline_done: false,
            feedback: String::new(),
            failure: None,
            import_kind: "progress".into(),
            preview: None,
            archive: None,
        }
    }
    pub fn is_legacy(&self) -> bool {
        self.workspace.progress.current.lesson == LESSON_ID
    }
    pub fn activity(&self) -> Option<&Activity> {
        let c = &self.workspace.progress.current;
        self.course.activity(&c.lesson, &c.activity)
    }
    pub fn lesson_title(&self) -> String {
        self.course
            .lesson(&self.workspace.progress.current.lesson)
            .map(|l| l.title.clone())
            .unwrap_or_default()
    }
    pub fn progress(&self) -> ActivityProgress {
        let c = &self.workspace.progress.current;
        self.workspace
            .progress
            .activity(&self.course, &c.lesson, &c.activity)
            .cloned()
            .unwrap_or_default()
    }
    pub fn progress_mut(&mut self) -> &mut ActivityProgress {
        let c = self.workspace.progress.current.clone();
        self.workspace
            .progress
            .activity_mut(&self.course, &c.lesson, &c.activity)
            .expect("selected authored objective")
    }
    pub fn selected(&self) -> Option<&OperationRun> {
        self.selected_operation
            .and_then(|i| self.operation_runs.get(i))
    }
    pub fn retain(&mut self, run: OperationRun) {
        self.operation_runs.push(run);
        let mut bytes: usize = self
            .operation_runs
            .iter()
            .map(|r| serde_json::to_vec(r).map_or(0, |s| s.len()))
            .sum();
        while self.operation_runs.len() > 1
            && (self.operation_runs.len() > 8 || bytes > 64 * 1024 * 1024)
        {
            bytes = bytes.saturating_sub(
                serde_json::to_vec(&self.operation_runs.remove(0)).map_or(0, |s| s.len()),
            );
            self.feedback="Older operation records were removed at the 8-run / 64 MiB limit. Export records before running more operations.".into();
        }
        self.selected_operation = Some(self.operation_runs.len() - 1);
    }
}

pub fn selection_from_hash(hash: &str) -> Option<(String, String)> {
    let route = hash.trim_start_matches('#').split('?').next()?;
    let parts = route.split('/').collect::<Vec<_>>();
    (parts.len() >= 5 && parts[1] == "lesson" && parts[3] == "activity")
        .then(|| (parts[2].into(), parts[4].into()))
}
pub fn open_activity(mut state: Signal<AppState>, lesson: &str, activity: &str, reset: bool) {
    if state.peek().busy {
        return;
    }
    let Some(definition) = state
        .peek()
        .learning
        .course
        .activity(lesson, activity)
        .cloned()
    else {
        state.write().notice = "This lesson link is not part of the installed course.".into();
        return;
    };
    if lesson == LESSON_ID {
        if let Some(i) = RELEASE_OBJECTIVES.iter().position(|id| *id == activity) {
            open_step(state, i + 1, true);
        }
        return;
    }
    crate::local::capture(state);
    let project_id =
        crate::local::project_id_for_selection(&state.peek().learning.workspace, lesson, activity);
    let existing = if reset {
        None
    } else {
        state
            .peek()
            .learning
            .workspace
            .projects
            .get(&project_id)
            .cloned()
    };
    {
        let mut s = state.write();
        s.learning.workspace.progress.current = LearningSelection {
            lesson: lesson.into(),
            activity: activity.into(),
        };
        s.learning.workspace.active_project = Some(project_id);
        s.learning.action_cursor = 0;
        s.learning.baseline_done = false;
        s.learning.feedback.clear();
        s.learning.candidate = None;
        s.learning.apply_dialog = false;
        s.learning.selected_operation = None;
        s.screen = Screen::Lesson;
        s.pane = Pane::Knowledge;
        s.selected_run = None;
        s.show_change = false;
        s.selected_event = 0;
        s.diagnostic = None;
        s.refusal = None;
        s.notice = if existing.is_some() {
            "Restored this activity’s local project. Actions run in authored order; reset to repeat from the original setup.".into()
        } else {
            "Opened the authored setup. Navigation remains unlocked.".into()
        };
    }
    update_route(state);
    let command = existing
        .map(|p| Command::Restore { session: p.session })
        .unwrap_or(Command::InitializeProfile {
            epoch: epoch(),
            statements: definition.setup,
            profile: definition.profile,
            recording: RecordingLevel::Full,
        });
    send(state, command, Purpose::Initialize);
}

#[component]
pub fn CoursePage() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let completed = s.learning.workspace.progress.completed(&s.learning.course);
    let total: usize = s
        .learning
        .course
        .lessons
        .iter()
        .map(|l| l.activities.len())
        .sum();
    let search = s.learning.search.to_lowercase();
    let lessons = s
        .learning
        .course
        .lessons
        .iter()
        .filter(|l| {
            (s.learning.track == "all" || s.learning.track == l.track)
                && (search.is_empty()
                    || format!(
                        "{} {} {}",
                        l.title,
                        l.summary,
                        l.activities
                            .iter()
                            .map(|a| a.title.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                    .to_lowercase()
                    .contains(&search))
        })
        .cloned()
        .collect::<Vec<_>>();
    rsx! {main{id:"main-content",tabindex:"-1",class:"course-page",
        div{class:"course-hero",div{span{class:"eyebrow","Learn by investigating"}h1{"From a plausible answer\nto a reasoned decision."}p{"Predict, inspect and repair with the real Nibli engine. Original lessons connect symbolic reasoning with the choices behind a neuro-symbolic application."}div{class:"button-row",button{class:"button primary",disabled:s.busy,onclick:move |_|{let c=state.peek().learning.workspace.progress.current.clone();open_activity(state,&c.lesson,&c.activity,false);},"Continue learning →"}button{class:"button",onclick:move |_|navigate(state,Screen::Packs),"Offline packs"}}}div{class:"course-progress",span{class:"hero-symbol","∴"}strong{"{completed} / {total}"}span{"activities checked"}p{"Your pace. Open navigation. Local progress."}}}
        section{class:"course-catalog","aria-label":"Course lessons",div{class:"course-filter",div{class:"segmented",for (id,label) in [("all","All lessons"),("foundations","Foundations"),("use","Use Nibli"),("integrate","Integrate applications"),("build","Build a reasoner")] {button{class:if s.learning.track==id{"selected"}else{""},onclick:move |_|state.write().learning.track=id.into(),"{label}"}}}label{class:"search-label","Find a lesson" input{r#type:"search",value:"{s.learning.search}",oninput:move |e|state.write().learning.search=e.value(),placeholder:"Scope, witnesses, evidence…"}}}
        for lesson in lessons { {let lid=lesson.id.clone();let first=lesson.activities[0].id.clone(); rsx!{article{class:"course-unit", "data-testid":"unit-{lesson.unit}",div{class:"unit-heading",span{class:"unit-number","{lesson.unit}"}div{span{class:"eyebrow","{lesson.track}"}h2{"{lesson.title}"}p{"{lesson.summary}"}}button{class:"button small",disabled:s.busy,onclick:move |_|open_activity(state,&lid,&first,false),"Open unit →"}}div{class:"activity-list",for activity in lesson.activities { {let checked=s.learning.workspace.progress.activity(&s.learning.course,&lesson.id,&activity.id).is_some_and(|p|p.checked);let l=lesson.id.clone();let a=activity.id.clone();rsx!{button{class:"activity-link",disabled:s.busy,"data-testid":"activity-{activity.id}",onclick:move |_|open_activity(state,&l,&a,false),span{class:if checked{"mint"}else{"muted"},if checked{"✓"}else{"○"}}span{"{activity.title}"}span{class:"muted","→"}}}}}}}}}}
        if s.learning.course.lessons.is_empty(){p{"No lessons are available in this course."}}
        }section{class:"future-tracks",h2{"Continue beyond the foundations"}p{"Build a reasoner through the compiler, store, proof and runtime labs. Specialist studios follow separately; independent domain review precedes their publication."}div{class:"button-row",button{class:"button",onclick:move |_|navigate(state,Screen::Reference),"Explore {s.learning.references.entries.len()} references"}button{class:"button",onclick:move |_|navigate(state,Screen::Data),"Projects and local data"}}}
    }}
}

#[component]
pub fn ActivityWorkspace() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let Some(a) = s.learning.activity().cloned() else {
        return rsx! {p{"Choose an activity from the course."}};
    };
    let current = s.learning.workspace.progress.current.clone();
    let lesson = s.learning.course.lesson(&current.lesson).unwrap().clone();
    let progress = s.learning.progress();
    let next_index = lesson
        .activities
        .iter()
        .position(|x| x.id == a.id)
        .unwrap_or(0)
        + 1;
    rsx! {div{class:"reading-notice",strong{"Reading mode on this screen"}"Lessons, references and saved result text remain available. Editing and running need a window at least 768 px wide."}
        main{id:"main-content",tabindex:"-1",class:"lesson-frame curriculum-frame",
            nav{class:"rail","aria-label":"Unit activities",span{class:"eyebrow rail-heading","{lesson.unit} · {lesson.title}"}div{class:"steps",for(i,item)in lesson.activities.iter().enumerate(){ {let l=lesson.id.clone();let id=item.id.clone();rsx!{button{class:if item.id==a.id{"step-button current"}else{"step-button"},"aria-current":if item.id==a.id{"step"}else{"false"},disabled:s.busy,onclick:move |_|open_activity(state,&l,&id,false),span{class:"step-number","{i+1}"}span{class:"step-label","{item.title}"}}}}}p{class:"rail-note","Every activity is available. Reset restores its authored setup."}}}
            aside{class:"lesson-panel",span{class:"eyebrow step-kicker","{lesson.unit} · Activity {next_index} of {lesson.activities.len()}"}h1{"{a.title}"}span{class:"eyebrow","Objective"}p{class:"objective","{a.objective}"}p{class:"lesson-explanation","{a.explanation}"}ul{class:"task-list",for task in &a.tasks{li{span{class:"task-icon","·"}span{"{task}"}}}}
                div{class:"terms",for id in &a.references{ {let target=id.clone();let title=s.learning.references.entries.iter().find(|entry|entry.id==*id).map(|entry|entry.title.clone()).unwrap_or_else(||id.clone());rsx!{button{class:"chip",onclick:move |_|{state.write().reference=target.clone();navigate(state,Screen::Reference);},"{title}"}}}}}
                span{class:"eyebrow","Help · authored"}button{class:"text-action",disabled:progress.hints_revealed>=a.hints.len(),onclick:move |_|{state.write().learning.progress_mut().hints_revealed+=1;persist(state);},"Reveal next hint"}
                for hint in a.hints.iter().take(progress.hints_revealed){div{class:"hint-box","{hint}"}}
                button{class:"text-action",onclick:move |_|{state.write().learning.progress_mut().solution_revealed=true;persist(state);},"Show the reasoning"}if progress.solution_revealed{div{class:"hint-box","{a.solution}"}}
                if let Some(q)=a.question.clone(){section{class:"knowledge-question",h3{"{q.prompt}"}for(i,choice)in q.choices.iter().enumerate(){label{class:"choice",input{r#type:"radio",name:"authored-question",checked:progress.answer==Some(i),onchange:move |_|{state.write().learning.progress_mut().answer=Some(i);persist(state);}}span{"{choice}"}}}if let Some(answer)=progress.answer{p{class:"hint-box",if answer==q.correct{"{q.feedback}"}else{"Compare the source, query and operation result, then try again."}}}}}
                if !a.self_review.is_empty(){section{class:"self-review",h3{"Self-review · your assessment"}ul{for criterion in &a.self_review{li{"{criterion}"}}}label{class:"reflection-label interactive-only","Your reasoning and plan (optional)" textarea{rows:"6",maxlength:"2048",value:progress.reflection.clone().unwrap_or_default(),"aria-describedby":"reflection-help",oninput:move|e|{let mut response=e.value();while response.len()>8192{response.pop();}state.write().learning.progress_mut().reflection=Some(response);persist(state);},placeholder:"Record your proposal, assumptions, maintenance owner and next test."}}p{id:"reflection-help",class:"small muted","Learner-authored and saved locally. Review your writing against the criteria; no model grades it."}if let Some(reflection)=&progress.reflection{div{class:"narrow-only reflection-reading",h4{"Your reasoning and plan"}p{"{reflection}"}}}label{class:"choice",input{r#type:"checkbox",checked:progress.self_reviewed,onchange:move |e|{state.write().learning.progress_mut().self_reviewed=e.checked();persist(state);}}"I can explain these points using the evidence available."}}}
                button{class:"button primary","data-testid":"check-activity",onclick:move |_|crate::operations::check_activity(state),if progress.checked{"✓ Activity checked"}else{"Check this activity"}}
                if !s.learning.feedback.is_empty(){p{class:"hint-box",role:"status","{s.learning.feedback}"}}
                div{class:"lesson-nav",button{class:"button quiet small",disabled:s.busy,onclick:{let l=lesson.id.clone();let id=a.id.clone();move |_|open_activity(state,&l,&id,true)},"Reset activity"}if let Some(next)=lesson.activities.get(next_index){ {let l=lesson.id.clone();let id=next.id.clone();rsx!{button{class:"button primary",disabled:s.busy,onclick:move |_|open_activity(state,&l,&id,false),"Next activity →"}}}}else{button{class:"button primary",onclick:move |_|navigate(state,Screen::Course),"Back to course →"}}}
            }
            section{class:"workspace curriculum-workspace","aria-label":"Reasoning workspace",
                crate::operations::OperationToolbar {}
                div{class:"workspace-content",if !s.notice.is_empty(){div{class:"notice","{s.notice}"}}if !s.storage_error.is_empty(){div{class:"notice error","{s.storage_error}"}}crate::operations::ActivityControls {}crate::operations::OperationResultView {}crate::operations::CandidateView {}crate::operations::AcceptedSources {}crate::integration::RetrievalPanel {}crate::integration::ReviewPanel {}crate::integration::CostPanel {}crate::integration::LabPanel {}}
            }
        }
    }
}

#[component]
pub fn ReferencePage() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let search = s.learning.reference_search.to_lowercase();
    let entries = s
        .learning
        .references
        .entries
        .iter()
        .filter(|r| {
            format!("{} {} {}", r.id, r.title, r.body)
                .to_lowercase()
                .contains(&search)
        })
        .cloned()
        .collect::<Vec<_>>();
    let current = s
        .learning
        .references
        .entries
        .iter()
        .find(|r| r.id == s.reference)
        .cloned()
        .or_else(|| entries.first().cloned());
    rsx! {main{id:"main-content",tabindex:"-1",class:"reference-page",div{class:"page-heading",span{class:"eyebrow","Reference · original explanations"}h1{"Keep the meaning in view."}p{"Search the notation, engine behavior and boundaries. Compile examples against the actual engine."}}
        div{class:"reference-layout",aside{class:"reference-index",label{"Search references" input{r#type:"search",value:"{s.learning.reference_search}",oninput:move|e|state.write().learning.reference_search=e.value(),placeholder:"Negation, count, recovery…"}}nav{"aria-label":"References",for item in entries{ {let id=item.id.clone();rsx!{button{class:if item.id==s.reference{"active"}else{""},onclick:move |_|state.write().reference=id.clone(),span{class:"eyebrow","{item.category}"}span{"{item.title}"}}}}}}}
            article{class:"reference-article",if let Some(entry)=current{span{class:"eyebrow","{entry.category}"}h2{"{entry.title}"}p{"{entry.body}"}for example in entry.examples{div{class:"reference-example",pre{"{example.source}"}p{"{example.explanation}"}if example.kind==ExampleKind::Kr{button{class:"button small interactive-only",onclick:move |_|state.write().learning.compile_source=example.source.clone(),"Use in compiler below"}}else{p{class:"small muted","Run context: {example.run_in.clone().unwrap_or_default()}"}}}}div{class:"terms",for related in entry.related{ {let id=related.clone();rsx!{button{class:"chip",onclick:move |_|state.write().reference=id.clone(),"{related}"}}}}}for link in entry.links{a{href:"{link.url}",target:"_blank",rel:"noreferrer","{link.label} ↗"}}}
                section{class:"grammar-workbench interactive-only",h2{"Try the actual compiler"}p{"Compilation returns the engine’s artifacts. It does not admit knowledge or answer a truth claim."}label{"KR source" textarea{rows:"5",value:"{s.learning.compile_source}",oninput:move |e|state.write().learning.compile_source=e.value(),spellcheck:"false"}}button{class:"button primary",disabled:s.busy,onclick:move |_|{let source=state.peek().learning.compile_source.clone();crate::operations::execute_free(state,Operation::Compile{source,structural_preflight:true});},"Compile and inspect"}}
                crate::operations::OperationResultView {}
            }
        }
    }}
}

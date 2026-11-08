use dioxus::prelude::*;
use nibli_learn_model::*;

mod curriculum;
mod inspectors;
mod integration;
mod lesson;
mod local;
mod offline;
mod operations;
mod state;
use state::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let state = use_signal(AppState::new);
    use_context_provider(|| state);
    use_hook(move || std::rc::Rc::new(local::subscribe(state)));
    #[cfg(target_arch = "wasm32")]
    use_hook(move || std::rc::Rc::new(install_navigation(state)));
    use_future(move || async move {
        boot(state);
    });
    let s = state.read();
    let announcement = if let Some(error) = &s.diagnostic {
        error.message.clone()
    } else if s.busy {
        if s.operation == "recovering" {
            "Restoring the last committed knowledge.".into()
        } else {
            "The reasoning worker is running.".into()
        }
    } else if s.operation == "candidate_complete" {
        s.learning
            .candidate
            .as_ref()
            .and_then(|c| c.run.as_ref())
            .map(|r| {
                format!(
                    "Detached candidate completed. {}. Main-project evidence is unchanged.",
                    crate::operations::announce(&r.result)
                )
            })
            .unwrap_or_else(|| "Candidate operation completed.".into())
    } else if (!s.learning.is_legacy() || s.screen == Screen::Reference)
        && s.learning.selected().is_some()
    {
        crate::operations::announce(&s.learning.selected().unwrap().result)
    } else if let Some(run) = s.run() {
        format!(
            "Run complete. Engine outcome {}. Recording {}.",
            run.outcome.status,
            run.recording_status()
        )
    } else {
        format!("Worker {}.", s.operation)
    };
    rsx! {
        document::Title { "Nibli Learn · learn by investigating" }
        document::Link { rel: "stylesheet", href: runtime_asset_url("assets/app.css") }
        div { class: "app", "data-theme": "{s.theme}",
            a { class: "skip-link", href: "#main-content", "Skip to learning workspace" }
            TopBar {}
            div { class: "sr-only", role: "status", "aria-live": "polite", "aria-atomic": "true", "{announcement}" }
            match s.screen {
                Screen::Course => rsx!{ curriculum::CoursePage {} },
                Screen::Lesson if !s.learning.is_legacy() => rsx!{ curriculum::ActivityWorkspace {} },
                Screen::Lesson | Screen::Sandbox => rsx!{ LessonWorkspace {} },
                Screen::Reference => rsx!{ curriculum::ReferencePage {} },
                Screen::Data => rsx!{ local::DataPage {} },
                Screen::Packs => rsx!{ offline::OfflinePage {} },
            }
            Footer {}
            if s.drawer.is_some() { Drawer {} }
            if s.reset_dialog { ResetDialog {} }
            if s.learning.apply_dialog { operations::ApplyDialog {} }
        }
    }
}

#[component]
fn TopBar() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let light = s.theme == "light";
    let saved = !s.storage_locked
        && s.storage_error.is_empty()
        && !s.learning.save_busy
        && !s.learning.save_dirty;
    rsx! {
        header { class: "topbar",
            a { class: "brand", href: "#/course", onclick: move |_| navigate(state,Screen::Course),
                span { class: "brand-mark", "aria-hidden": "true", "∴" } "nibli learn"
            }
            nav { class: "breadcrumbs", "aria-label": "Breadcrumb",
                a { href: "#/course", onclick: move |_| navigate(state,Screen::Course), "Course" }
                if s.screen == Screen::Lesson { " / {s.learning.lesson_title()}" }
                if s.screen == Screen::Sandbox { " / Open sandbox" }
            }
            div { class: "top-actions",
                button { class: "button quiet optional-link", id: "reference-trigger", onclick: move |_| { if state.peek().learning.is_legacy() && matches!(state.peek().screen,Screen::Lesson|Screen::Sandbox){state.write().drawer=Some("reference".into());}else{navigate(state,Screen::Reference);} }, "Reference" }
                button { class: "button quiet optional-link", id: "tutor-trigger", onclick: move |_| { state.write().drawer=Some("tutor".into()); }, "Tutor · optional" }
                button { class: "button quiet small", title: if light { "Switch to dark theme" } else { "Switch to light theme" }, "data-testid": "theme-toggle", onclick: move |_| { state.write().theme=if light {"dark".into()} else {"light".into()}; persist(state); }, if light { "◐ Dark" } else { "◑ Light" } }
                button { class: if saved { "badge ok" } else { "badge warn" }, onclick: move |_| navigate(state,Screen::Data),
                    span { class: "dot", "aria-hidden": "true" } if saved { "Saved on this device" } else if s.learning.save_busy { "Saving locally…" } else { "Not saved · export work" }
                }
            }
        }
    }
}

#[component]
fn Footer() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    let worker_status = if s.busy {
        if s.operation == "recovering" {
            "recovering"
        } else {
            "busy"
        }
    } else if s.operation == "error" {
        "error"
    } else {
        "ready"
    };
    rsx! {
        footer { class: "footer",
            span { class: "runtime", "data-testid": "worker-status", "data-state": "{worker_status}",
                if s.busy { span { class: "loading-spinner", "aria-hidden":"true" } } else { span { class: if s.operation=="error"{"dot error-text"}else{"dot mint"}, "aria-hidden":"true" } }
                if s.busy { "Worker {s.operation}" } else if s.operation=="error" {"Engine unavailable · restore session"}else{ "Engine ready · in this browser" }
            }
            span { "Input revision {s.revision()}" }
            span { "Closed world" }
            a { href: "#/data", onclick: move |_| navigate(state,Screen::Data), "Local data · no account · no analytics" }
        }
    }
}

#[component]
fn LessonWorkspace() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    let sandbox = s.screen == Screen::Sandbox;
    rsx! {
        div { class:"reading-notice", strong { "Reading mode on this screen" }
            "Read lessons, statements and saved results here. Editing, running, replay and proof inspection need a window at least 768 px wide."
        }
        main { id:"main-content",tabindex:"-1", class:"lesson-frame",
            if !sandbox { StepRail {} LessonPanel {} }
            section { class:"workspace", "aria-label":"Reasoning workspace",
                if s.complete && !sandbox { Completion {} }
                else {
                    WorkspaceToolbar {}
                    if s.show_settings { RunSettingsPanel {} }
                    if !s.exercise_open && !sandbox && s.step()!=4 && s.step()!=6 && s.step()!=7 { Prediction {} }
                    if !s.exercise_open { StatusBand {} RunHistory {} }
                    div { class:"workspace-content",
                        if !s.notice.is_empty() { div { class:"notice", "{s.notice}" } }
                        if !s.storage_error.is_empty() { div { class:"notice error", "{s.storage_error}" button { class:"button small", onclick:move |_| navigate(state,Screen::Data), "Export local work" } } }
                        if s.exercise_open { RepairExercise {} }
                        else { match s.pane {
                            Pane::Knowledge=>rsx!{ Knowledge {} integration::ReviewPanel {} },
                            Pane::Replay=>rsx!{div{class:"interactive-only",inspectors::Replay {}} NarrowRunReading {}},
                            Pane::Proof=>rsx!{div{class:"interactive-only",inspectors::Proof {}} NarrowRunReading {}},
                        } }
                    }
                }
            }
        }
    }
}

#[component]
fn StepRail() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    let current = s.step();
    rsx! {
        nav { class:"rail", "aria-label":"Lesson steps",
            span { class:"eyebrow rail-heading", "Release readiness" }
            div { class:"steps",
                for (i,step) in lesson::STEPS.iter().enumerate() {
                    button { key:"{i}", class:if current==i+1 {"step-button current"} else if s.progress.steps[i].checked {"step-button done"} else {"step-button"},
                        "aria-current":if current==i+1 {"step"} else {"false"}, "aria-label":"Step {i+1}: {step.title}", "data-testid":"step-{i+1}", disabled:s.busy,
                        onclick:move |_| open_step(state,i+1,true),
                        span { class:"step-number", "{i+1}" } span { class:"step-label", "{step.title}" }
                        if s.progress.steps[i].checked { span { class:"step-done", "aria-label":"Checked", "✓" } }
                    }
                }
            }
            p { class:"rail-note", "Steps are never locked. Opening one out of order starts it from its authored setup." }
        }
    }
}

#[component]
fn NarrowRunReading() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    rsx! {section{class:"narrow-only",h2{"Saved result"}
        if let Some(run)=s.run(){p{class:"explanation-line",style:"margin-top:16px","{run.why}"}details{summary{"Read proof text"}pre{style:"margin-top:14px","{run.proof_text}"}}}
        else{p{style:"margin-top:16px","No local run is selected. Return to Knowledge to read the accepted statements and this step's explanation."}}
        button{class:"button",style:"margin-top:20px",onclick:move |_|open_pane(state,Pane::Knowledge),"Read accepted knowledge"}
    }}
}

#[component]
fn LessonPanel() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let n = s.step();
    let step = &lesson::STEPS[n - 1];
    let progress = &s.progress.steps[n - 1];
    let feedback = s.saved_run().filter(|r| r.step == n).and_then(|r| {
        r.prediction.as_ref().map(|p| {
            (
                p == &r.run.outcome.status,
                p.clone(),
                r.run.outcome.status.clone(),
            )
        })
    });
    let support_detail = match s.support_stage {
        0 => "5a · Add another review",
        1 => "5b · Withdraw one review",
        _ => "5c · Withdraw the last review",
    };
    rsx! {
        aside { class:"lesson-panel", "aria-label":"Lesson and guidance",
            span { class:"eyebrow step-kicker", "Step {n} of 7" }
            h1 { "{step.title}" }
            span { class:"eyebrow", "Objective" }
            p { class:"objective", "{step.objective}" }
            p { class:"lesson-explanation", "{step.explanation}" }
            div { class:"terms", span { class:"eyebrow", style:"margin:0", "Terms" }
                button { class:"chip", onclick:move |_| {let mut s=state.write();s.drawer=Some("reference".into());s.reference="derivable".into();}, "derivable" }
                button { class:"chip", onclick:move |_| {let mut s=state.write();s.drawer=Some("reference".into());s.reference="assertion".into();}, "assertion" }
            }
            if n==5 { div { class:"notice", "{support_detail}" } }
            span { class:"eyebrow", "In this step" }
            ul { class:"task-list", for task in step.tasks { li { span { class:"task-icon", "aria-hidden":"true", if progress.checked {"✓"} else {"·"} } span { "{task}" } } } }
            if let Some((matched,predicted,actual))=feedback { if !s.stale() && n!=4 && n!=6 && n!=7 {
                div { class:if matched {"feedback match"} else {"feedback miss"},
                    h3 { if matched {"✓ Your prediction matches"} else {"≠ Your prediction differs"} }
                    p { "You predicted {predicted}. The actual engine outcome was {actual}." }
                    if !matched { p { "Inspect the accepted evidence and the recorded attempt, then try again." } }
                }
            } }
            if n==4 || n==6 { UnderstandingCheck {} }
            span { class:"eyebrow", "Help · authored" }
            div { class:"button-row",
                button { class:"text-action", onclick:move |_| {state.write().progress.steps[n-1].hint_revealed=true;persist(state);}, "Show a hint" }
                button { class:"text-action", onclick:move |_| {state.write().progress.steps[n-1].solution_revealed=true;persist(state);}, "Show the reasoning" }
            }
            if progress.hint_revealed { div { class:"hint-box", "{step.hint}" } }
            if progress.solution_revealed { div { class:"hint-box", "{step.solution}" } }
            div { class:"lesson-bottom" }
            nav { class:"lesson-nav", "aria-label":"Step navigation",
                button { class:"button quiet small", disabled:n==1||s.busy, onclick:move |_| open_step(state,n.saturating_sub(1),true), "← Previous" }
                button { class:"button quiet small", id:"lesson-reset", "data-testid":"lesson-reset", disabled:s.busy, onclick:move |_| state.write().reset_dialog=true, "Reset step" }
                if n<7 { button { class:"button primary", "data-testid":"next-step", disabled:s.busy, onclick:move |_| open_step(state,n+1,false), "Next step →" } }
                else { button { class:"button primary", disabled:s.busy, onclick:move |_| state.write().complete=true, "Review lesson →" } }
            }
        }
    }
}

#[component]
fn UnderstandingCheck() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let n = s.step();
    let options = if n == 4 {
        [
            "Replay shows attempted operations; proof explains the returned derivation.",
            "The proof lists every operation the engine attempted.",
            "Playing a saved recording runs the reasoner again.",
        ]
    } else {
        [
            "The direct assertion was refused; committed knowledge did not change.",
            "FALSE means the attempted statement was admitted.",
            "The refusal proves the release is unsafe.",
        ]
    };
    rsx! {
        div { class:"feedback interactive-only",
            h3 { "Check your understanding" }
            if n==4 && (!s.viewed_replay || !s.viewed_proof) { p { class:"small muted", "Inspect both Replay and Proof before checking your explanation." } }
            if n==6 && s.refusal.is_none() { p { class:"small muted", "Try the protected assertion in the workspace first." } }
            for (i,label) in options.into_iter().enumerate() {
                label { class:"prediction-option", style:"margin-top:8px;min-height:44px",
                    input { r#type:"radio", name:"understanding", checked:s.quiz==Some(i), "data-testid":"quiz-{i}", oninput:move |_| state.write().quiz=Some(i) }
                    span { class:"small", "{label}" }
                }
            }
            button { class:"button small", style:"margin-top:12px", "data-testid":"check-understanding", onclick:move |_| {
                let mut s=state.write();
                let explored=if n==4 {s.viewed_replay&&s.viewed_proof&&s.run().is_some()} else {s.refusal.is_some()};
                if !explored {s.quiz_feedback="Complete the inspection above, then check your explanation.".into();}
                else if s.quiz==Some(0) {s.progress.steps[n-1].checked=true;s.quiz_feedback="Correct. You identified what this result establishes.".into();}
                else {s.quiz_feedback="Not yet. Use the authored hint and inspect the relevant view.".into();}
                drop(s);persist(state);
            }, "Check understanding" }
            if !s.quiz_feedback.is_empty() { p { class:"small", style:"margin-top:12px", "{s.quiz_feedback}" } }
        }
    }
}

#[component]
fn WorkspaceToolbar() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let sandbox = s.screen == Screen::Sandbox;
    rsx! {
        div { class:"workspace-toolbar",
            div { class:"tabs", role:"tablist", "aria-label":"Workspace view",onkeydown:move |e|{
                let current=match state.peek().pane{Pane::Knowledge=>0,Pane::Replay=>1,Pane::Proof=>2};
                let next=match e.key(){Key::ArrowRight=>Some((current+1)%3),Key::ArrowLeft=>Some((current+2)%3),Key::Home=>Some(0),Key::End=>Some(2),_=>None};
                if let Some(next)=next{e.prevent_default();open_pane(state,[Pane::Knowledge,Pane::Replay,Pane::Proof][next]);focus(["knowledge-tab","replay-tab","proof-tab"][next]);}
            },
                button { id:"knowledge-tab",tabindex:if s.pane==Pane::Knowledge{"0"}else{"-1"},class:if s.pane==Pane::Knowledge {"tab active"}else{"tab"}, role:"tab", "aria-selected":s.pane==Pane::Knowledge, "data-testid":"knowledge-tab", onclick:move |_| open_pane(state,Pane::Knowledge), "Knowledge" }
                button { id:"replay-tab",tabindex:if s.pane==Pane::Replay{"0"}else{"-1"},class:if s.pane==Pane::Replay {"tab active"}else{"tab"}, role:"tab", "aria-selected":s.pane==Pane::Replay, "data-testid":"replay-tab", onclick:move |_| open_pane(state,Pane::Replay), "Replay" span { class:"tab-count", "{s.events().len()}" } }
                button { id:"proof-tab",tabindex:if s.pane==Pane::Proof{"0"}else{"-1"},class:if s.pane==Pane::Proof {"tab active"}else{"tab"}, role:"tab", "aria-selected":s.pane==Pane::Proof, "data-testid":"proof-tab", onclick:move |_| open_pane(state,Pane::Proof), "Proof" }
            }
            div { class:"segmented", "aria-label":"Notation view",
                for mode in ["plain","both","formal"] { button { "aria-pressed":s.notation==mode, onclick:move |_| state.write().notation=mode.into(), "{mode}" } }
            }
            button { class:"button quiet small interactive-only", "aria-expanded":s.show_settings, onclick:move |_| {let show=state.peek().show_settings;state.write().show_settings= !show;}, "Run settings" }
            if sandbox || s.step()==4 { button { class:"button primary small interactive-only", "data-testid":"run-claim", disabled:s.busy, onclick:move |_| run_claim(state), "Run claim" } }
        }
    }
}

#[component]
fn RunSettingsPanel() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    let settings = s.settings();
    rsx! {
        div { class:"settings-popover", div { class:"settings-grid",
            label { "Search depth", select { "data-testid":"depth-setting", value:"{settings.depth}", disabled:s.busy, onchange:move |event| {
                if let Ok(depth)=event.value().parse() {let mut settings=state.peek().settings();settings.depth=depth;send(state,Command::Settings{settings},Purpose::Settings);}
            }, option{value:"10","Lesson default · 10"} option{value:"20","2× · 20"} option{value:"40","4× · 40"} option{value:"1","1 · inspect a depth boundary"} } }
            label { "Recording", select { value:if settings.recording==RecordingLevel::Full {"full"}else{"standard"}, disabled:s.busy, onchange:move |event| {
                let mut settings=state.peek().settings();settings.recording=if event.value()=="full"{RecordingLevel::Full}else{RecordingLevel::Standard};send(state,Command::Settings{settings},Purpose::Settings);
            }, option{value:"standard","Standard"} option{value:"full","Full detail"} } }
        } p { class:"small muted", style:"margin:12px 0 0", "A settings change creates a new input revision. Earlier runs retain their original settings. Browser runs expose depth limits; host fuel and memory limits belong to local labs." } }
    }
}

#[component]
fn Prediction() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let prediction = s.prediction();
    let hint = if s.busy {
        "The reasoning worker is running"
    } else if !s.change_done() {
        "Make this step's change first"
    } else if prediction.is_none() {
        "Choose a prediction first"
    } else {
        "Runs these inputs in this browser"
    };
    rsx! {
        section { class:"prediction interactive-only", "aria-label":"Predict then run",
            span { class:"eyebrow", "Predict, then run" }
            fieldset { legend { "Does “BuildA is ready for release” follow from these inputs?" }
                div { class:"prediction-options",
                    for (value,meaning) in [("TRUE","It follows"),("FALSE","It doesn't follow"),("UNKNOWN","It can't be decided")] {
                        label { class:if prediction.as_deref()==Some(value){"prediction-option selected"}else{"prediction-option"},
                            input { r#type:"radio", name:"prediction", value:"{value}", checked:prediction.as_deref()==Some(value), disabled:s.busy, "data-testid":"prediction-{value.to_lowercase()}", oninput:move |_|select_prediction(state,value) }
                            span { span { class:"value","{value}" } span { class:"meaning","{meaning}" } }
                        }
                    }
                    div { class:"run-group",
                        if s.busy {button {class:"button", "data-testid":"run-cancel", onclick:move |_|cancel(state), "Cancel run"}}
                        else {button {class:"button primary", "data-testid":"run-claim", disabled:s.run_disabled(), "aria-describedby":"run-hint", onclick:move |_|run_claim(state), "Run claim"}}
                        span { id:"run-hint",class:"run-hint","{hint}" }
                    }
                }
            }
            details {class:"prediction-note",summary{"Add a note about your reasoning · optional"} textarea{value:"{s.note}","aria-label":"Prediction explanation",oninput:move|e|state.write().note=e.value()} }
        }
    }
}

#[component]
pub fn Verdict(status: String, detail: Option<String>) -> Element {
    let lower = status.to_lowercase();
    let (glyph, class) = match status.as_str() {
        "TRUE" => ("⊢", "true"),
        "FALSE" => ("⊬", "false"),
        "UNKNOWN" => ("◌", "unknown"),
        "RESOURCE_EXCEEDED" => ("⧗", "resource"),
        _ => ("—", "none"),
    };
    let label = if let Some(detail) = detail.filter(|d| !d.is_empty()) {
        format!("{status} ({detail})")
    } else {
        status.clone()
    };
    rsx! {span{class:"verdict {class}","data-status":"{status}","data-outcome":"{lower}",span{"aria-hidden":"true","{glyph}"}"{label}"}}
}

#[component]
fn StatusBand() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    let run = s.run();
    let stale = s.stale();
    let status = if s.busy {
        "Deciding".into()
    } else if s.operation == "cancelled" && run.is_none() {
        "No completed outcome".into()
    } else {
        run.map(|r| r.outcome.status.clone())
            .unwrap_or_else(|| "No result yet".into())
    };
    let detail = if s.busy {
        None
    } else {
        run.and_then(|r| r.outcome.detail.clone())
    };
    let prediction = s.saved_run().and_then(|r| r.prediction.as_ref());
    let matched = run.zip(prediction).map(|(r, p)| &r.outcome.status == p);
    let record_status = run.map(|r| r.recording_status()).unwrap_or("unavailable");
    let ordinal = s.saved_run().map(|r| r.ordinal).unwrap_or(0);
    let meaning = match status.as_str() {
        "TRUE" => "Derivable from this run's inputs.",
        "FALSE" => "Not derivable from this run's inputs.",
        "UNKNOWN" => "Unresolved under the reported condition.",
        "RESOURCE_EXCEEDED" => "The search reached its configured limit.",
        _ => "Run the claim to inspect an actual engine result.",
    };
    rsx! {
        section{class:"status-band","aria-label":"Run result and learning feedback",
            div{class:"status-slot","data-testid":"outcome","data-status":"{status}","data-run-id":run.map(|r|r.id.clone()).unwrap_or_default(),
                span{class:"eyebrow","Engine outcome"} Verdict{status:status.clone(),detail} p{"{meaning}"}
                if stale{p{class:"amber","From an earlier input revision. Rerun to evaluate the current knowledge."}}
                if let Some(r)=run{if r.outcome.cwa_false {p{class:"small muted","Closed world · not a proof of the opposite."}}if r.outcome.naf_dependent{p{class:"small muted","This derivation depends on absence."}}}
                div{class:"status-links interactive-only",button{class:"text-action",onclick:move |_|open_pane(state,Pane::Replay),"Inspect replay"}button{class:"text-action",onclick:move |_|open_pane(state,Pane::Proof),"Open proof"}}
            }
            div{class:"status-slot",
                span{class:"eyebrow","Learning check"}
                if stale{span{class:"badge warn","Earlier inputs"}p{"This run remains evidence of its original revision."}}
                else if (s.step()==4||s.step()==6)&&s.progress.steps[s.step()-1].checked{span{class:"badge ok","✓ Understood"}p{"This step's understanding check is complete."}}
                else if let Some(matched)=matched{span{class:if matched{"badge ok"}else{"badge warn"},if matched{"✓ Matches"}else{"≠ Doesn't match"}}p{"You predicted {prediction.cloned().unwrap_or_default()}."}}
                else{span{class:"badge","Not checked"}p{if s.step()==4||s.step()==6{"Use the understanding check beside the workspace."}else{"Choose a prediction before running."}}}
            }
            div{class:"status-slot",
                span{class:"eyebrow","Run · recording"}
                div{class:"button-row",if s.busy{span{class:"loading-spinner","aria-hidden":"true"}}else{span{class:"dot mint","aria-hidden":"true"}}strong{class:"small mono",if s.busy{"Worker {s.operation}"}else if s.operation=="cancelled"&&run.is_none(){"Cancelled · worker restored"}else if s.operation=="error"{"✗ Operational error"}else if run.is_some(){"Run #{ordinal} complete"}else{"Engine ready"}}}
                if let Some(r)=run{p{class:"muted","Recording {record_status}. Input revision {r.revision}."}
                    details{class:"run-details",summary{"Run details"}dl{dt{"Run"}dd{"{r.id}"}dt{"Session"}dd{"{r.epoch}"}dt{"Depth"}dd{"{r.settings.depth}"}dt{"Recording"}dd{"{record_status}"}dt{"Engine"}dd{ {r.record.get("engine_version").map(|v|v.to_string()).unwrap_or_else(||"See record export".into())} }}}
                }
                if s.busy{button{class:"button small interactive-only","data-testid":"run-cancel",onclick:move |_|cancel(state),"Cancel and restore"}}
                if run.is_some_and(|r|r.outcome.status=="RESOURCE_EXCEEDED"){button{class:"button small interactive-only",disabled:s.busy,onclick:move |_|{let mut settings=state.peek().settings();settings.depth=settings.depth.saturating_mul(2).min(1000);send(state,Command::Settings{settings},Purpose::Settings);},"Raise depth · then rerun"}}
                if s.operation=="error"{button{class:"button small",onclick:move |_|cancel(state),"Restore committed session"}}
            }
        }
    }
}

#[component]
fn RunHistory() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    rsx! {if !s.runs.is_empty()||!s.aborted.is_empty(){div{class:"history","aria-label":"Saved runs",span{class:"eyebrow","Runs"}
        for (i,saved) in s.runs.iter().enumerate().rev(){button{key:"{i}",title:"Run identity: {saved.run.id}",class:if s.selected_run==Some(i){"run-chip active"}else{"run-chip"},"aria-pressed":s.selected_run==Some(i),onclick:move |_|{let mut s=state.write();s.selected_run=Some(i);s.selected_event=0;s.show_change=false;s.selected_node=None;},span{class:"label","{saved.run.outcome.status}"}span{class:"revision","#{saved.ordinal} · revision {saved.run.revision}"}}}
        for attempt in s.aborted.iter().rev(){details{class:"run-chip","data-testid":"aborted-attempt",summary{class:"label","Aborted · revision {attempt.revision}"}p{class:"small","{attempt.claim} · depth {attempt.settings.depth}. No completed outcome or proof. Session {attempt.epoch}."}button{class:"text-action",onclick:{let attempt=attempt.clone();move |_|download("nibli-aborted-attempt.json",serde_json::to_string_pretty(&attempt).unwrap_or_default())},"Export metadata ↓"}}}
        span{class:"history-note","History keeps up to 8 runs or 64 MiB. Export records before running more queries to keep them permanently."}
    }}}
}

#[component]
fn Knowledge() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let sandbox = s.screen == Screen::Sandbox;
    let assertions = s
        .session
        .as_ref()
        .map(|s| s.assertions.clone())
        .unwrap_or_default();
    let policy = assertions.iter().find(|a| a.text == AND_POLICY && a.active);
    let evidence_count = assertions
        .iter()
        .filter(|a| {
            a.active && (sandbox || (!a.text.starts_with("derived_only") && a.text != AND_POLICY))
        })
        .count();
    let checks = s.active("approves(TestRunner, BuildA).");
    let reviews = s.active("approves(Reviewer, BuildA).");
    let suggested = match s.step() {
        2 => Some(("approves(Reviewer, BuildB).", "Review B1")),
        3 => Some(("approves(Reviewer, BuildA).", "Review A1")),
        5 if s.support_stage == 0 => Some(("approves(Reviewer, BuildA).", "Review A2")),
        6 => Some((CLAIM, "Direct readiness attempt")),
        _ => None,
    };
    rsx! {
        div{class:"panel-heading",span{class:"section-label","Knowledge · revision {s.revision()}" span{class:"chip","closed world"}}
            if !sandbox{button{class:"text-action interactive-only",onclick:move |_|{state.write().screen=Screen::Sandbox;state.write().notice="Sandbox opened from this lesson's current knowledge. Changes here are deliberate; reopening a lesson step can load its authored setup.".into();update_route(state);},"Open in sandbox →"}}
        }
        if s.stale(){div{class:"notice stale","The selected run belongs to earlier inputs. This pane shows current accepted knowledge."}}
        if !sandbox{
            div{class:"rule-summary",div{class:"section-label","What the policy needs"}
                div{class:"rule-flow",div{class:"rule-inputs",
                    div{class:if checks>0{"rule-box"}else{"rule-box empty"},span{class:"eyebrow","Condition 1"}code{"approves(TestRunner, BuildA)"}div{if checks>0{span{class:"mint","● {checks} active assertion"}}else{"○ No supporting assertion"}}}
                    div{class:if reviews>0{"rule-box"}else{"rule-box empty"},span{class:"eyebrow","Condition 2"}code{"approves(Reviewer, BuildA)"}div{if reviews>0{span{class:"mint","● {reviews} active assertion(s)"}}else{"○ No supporting assertion"}}}
                }span{class:"rule-arrow","aria-hidden":"true","→"}div{class:"rule-box",strong{"AND"}div{"both, same $build"}}span{class:"rule-arrow","aria-hidden":"true","→"}div{class:"rule-box result",code{"ready(BuildA, Release)"}}}
                p{"This diagram describes the policy and currently accepted inputs. Run the claim to determine its actual result."}
            }
            span{class:"section-label","Policy"}
            article{class:"statement",div{class:"statement-header",span{class:"badge","Rule"}span{class:"small muted",if policy.is_some(){"Active · lesson policy"}else{"Authored baseline · currently absent"}}}
                if s.notation!="formal"{p{"A build is ready for release when TestRunner and Reviewer have each approved that same build."}}
                if s.notation!="plain"{code{"{AND_POLICY}"}}
                if let Some(policy)=policy{div{class:"small muted mono",style:"margin-top:10px","Assertion #{policy.id} · {policy.source}"}}
                else{p{class:"notice",style:"margin-top:12px","This policy is no longer active in the accepted knowledge. Reset the step to restore its authored setup, or inspect your sandbox changes."}}
            }
            span{class:"section-label",style:"margin-top:26px","Claim under investigation"}
            article{class:"statement",div{class:"statement-header",span{class:"badge","Protected · derived-only"}}
                if s.notation!="formal"{p{"BuildA is ready for release."}}if s.notation!="plain"{code{"{CLAIM}"}}
            }
        }else{div{class:"field interactive-only",label{r#for:"sandbox-claim","Claim to investigate"}textarea{id:"sandbox-claim",value:"{s.sandbox_claim}",oninput:move|e|state.write().sandbox_claim=e.value()}}}
        div{class:"subheading",span{class:"section-label","Evidence · {evidence_count} active assertions"}
            if s.change_record.is_some(){button{class:"text-action interactive-only",onclick:move |_|{let mut s=state.write();s.show_change=true;s.selected_event=0;s.pane=Pane::Replay;},"Inspect last change →"}}
        }
        for assertion in assertions.iter().filter(|a|sandbox||(!a.text.starts_with("derived_only")&&a.text!=AND_POLICY)){
            AssertionRow{assertion:assertion.clone()}
        }
        if assertions.is_empty(){div{class:"empty",span{class:"empty-symbol","∴"}h2{"Accepted knowledge appears here"}p{"The browser worker is preparing the lesson."}}}
        if let Some(refusal)=&s.refusal{div{class:"notice refusal",id:"admission-refusal",tabindex:"-1","data-testid":"admission-refusal",strong{"⊘ Admission refused"}p{style:"margin-top:8px","{refusal}"}p{"Nothing from this attempted change was committed. Current revision: {s.revision()}."}
            div{class:"button-row",button{class:"button small refuse",onclick:move |_|{state.write().draft="approves(Reviewer, BuildA).".into();state.write().source="Review A3".into();focus("composer-text");},"Add supporting evidence instead"}}
        }}
        if s.step()==7&&!sandbox{div{class:"exercise-banner",h2{"A separate policy-repair exercise"}p{"The lesson knowledge is preserved while four isolated cases test your policy."}button{class:"button primary interactive-only",style:"margin-top:16px","data-testid":"open-exercise",onclick:move |_|{state.write().exercise_open=true;state.write().diagnostic=None;},"Open repair exercise →"}}}
        section{class:"composer interactive-only","aria-label":"Add an assertion",h3{"Add evidence"}
            p{class:"composer-help",style:"margin-top:8px","Write a formal statement and label its source. The engine validates admission."}
            if let Some((text,source))=suggested{button{class:"button small",style:"margin-bottom:14px","data-testid":"use-suggestion",onclick:move |_|{let mut s=state.write();s.draft=text.into();s.source=source.into();drop(s);focus("composer-text");},"Use this step's statement"}}
            div{class:"field",label{r#for:"composer-text","Statement"}textarea{id:"composer-text","data-testid":"composer-text",value:"{s.draft}",placeholder:"approves(Reviewer, BuildA).","aria-invalid":s.diagnostic.is_some()&&s.refusal.is_none(),oninput:move|e|{state.write().draft=e.value();state.write().diagnostic=None;}}}
            div{class:"split-fields",div{class:"field",label{r#for:"composer-source","Source label"}input{id:"composer-source","data-testid":"composer-source",r#type:"text",value:"{s.source}",placeholder:"Review note A1",oninput:move|e|state.write().source=e.value()}}
                button{class:"button primary","data-testid":"composer-assert",disabled:s.busy||s.draft.trim().is_empty()||s.source.trim().is_empty(),onclick:move |_|{let (text,source)={let s=state.peek();(s.draft.clone(),s.source.clone())};send(state,Command::Change{changes:vec![Change::Assert{text,source}]},Purpose::Change);},"Assert statement"}
            }
            if let Some(error)=&s.diagnostic{if s.refusal.is_none(){p{class:"diagnostic",role:"alert","{error.message}"}}}
            p{class:"small muted",style:"margin:12px 0 0","A source label records provenance. Review the source itself before treating it as evidence."}
        }
    }
}

#[component]
fn AssertionRow(assertion: Assertion) -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    let id = assertion.id.clone();
    let id2 = id.clone();
    let original = assertion.clone();
    let active = assertion.active;
    rsx! {article{class:if s.linked_assertion.as_deref()==Some(&id){"statement linked"}else if active{"statement"}else{"statement inactive"},id:"assertion-{id}","data-testid":"assertion-{id}","data-active":active,"data-statement":"{assertion.text}",
        div{class:"statement-header",span{class:"badge",if active{"Fact"}else{"Withdrawn"}}span{class:"small muted",if active{"Accepted assertion"}else{"Retained in assertion history"}}}
        if s.notation!="formal"{if let Some(plain)=assertion.plain{p{"{plain}"}}}
        if s.notation!="plain"{code{"{assertion.text}"}}
        div{class:"assertion-meta",span{"#{id}"}span{class:"source","{assertion.source}"}span{class:if active{"badge ok"}else{"badge"},if active{"● Active"}else{"Withdrawn"}}
            button{class:"text-action interactive-only","data-testid":"withdraw-{id}",disabled:s.busy,onclick:move |_|{
                let change=if active{Change::Withdraw{id:id2.clone()}}else{Change::Assert{text:original.text.clone(),source:original.source.clone()}};
                send(state,Command::Change{changes:vec![change]},Purpose::Change);
            },if active{"Withdraw"}else{"Assert again"}}
        }
    }}
}

#[component]
fn RepairExercise() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let stale = s.checks_revision.is_some_and(|r| r != s.policy_revision);
    let changed = s.policy_draft != s.policy;
    rsx! {
        section{"aria-label":"Policy repair exercise",div{class:"exercise-banner",span{class:"eyebrow","Separate exercise · revision {s.policy_revision}"}h2{"Both approvals. The same build."}p{"Your investigation's accepted knowledge stays unchanged. Each case runs in its own isolated engine session."}}
            button{class:"text-action",onclick:move |_|state.write().exercise_open=false,"← Back to the investigation"}
            div{class:"field interactive-only",style:"margin-top:22px",label{r#for:"policy-editor","Policy draft"}textarea{id:"policy-editor","data-testid":"policy-editor",value:"{s.policy_draft}",rows:"4",oninput:move|e|state.write().policy_draft=e.value()}}
            pre{class:"narrow-only code-block","{s.policy_draft}"}
            div{class:"button-row interactive-only",
                button{class:"button",disabled:s.busy||!changed,"data-testid":"apply-policy",onclick:move |_|{let mut s=state.write();s.policy=s.policy_draft.clone();s.policy_revision+=1;s.exercise_feedback="Policy draft updated. Run the cases to validate and evaluate it.".into();},"Use this policy draft"}
                button{class:"button primary","data-testid":"run-cases",disabled:s.busy||changed,onclick:move |_|{let policy=state.peek().policy.clone();send(state,Command::CheckRepair{policy},Purpose::Cases);},if s.busy{"Running cases…"}else{"Run all four cases"}}
            }
            if let Some(error)=&s.diagnostic{div{class:"notice error",style:"margin-top:16px",role:"alert","{error.message}"}}
            if stale{div{class:"notice stale",style:"margin-top:16px","These case results belong to the previous policy revision. Run again before checking the repair."}}
            table{class:"case-table","data-testid":"case-results",caption{class:"sr-only","Actual policy checks against four independent evidence cases"},thead{tr{th{"Case"}th{"Intended"}th{"Actual engine result"}th{"Check"}}}
                tbody{for (i,name) in ["Only TestRunner approves BuildA","Only Reviewer approves BuildA","TestRunner approves BuildA; Reviewer approves BuildB","Both approve BuildA"].into_iter().enumerate(){
                    tr{td{span{class:"small mono muted","{['A','B','C','D'][i]} · "}"{name}"}td{span{class:"mono",if i==3{"TRUE"}else{"FALSE"}}}td{if let Some(check)=s.checks.get(i){Verdict{status:check.actual.clone(),detail:check.run.outcome.detail.clone()}}else{span{class:"muted","Not run"}}}td{if let Some(check)=s.checks.get(i){span{class:if check.passed{"mint"}else{"amber"},if check.passed{"✓ Matches"}else{"≠ Differs"}}button{class:"text-action",style:"display:block;margin-top:8px",onclick:move |_|{
                        let check=state.peek().checks.get(i).cloned();if let Some(check)=check{let mut s=state.write();s.retain_run(SavedRun::new(check.run,None,7));s.exercise_open=false;s.pane=Pane::Proof;}
                    },"Inspect run"}}else{"—"}}}
                }}
            }
            if !s.exercise_feedback.is_empty(){div{class:"notice","{s.exercise_feedback}"}}
            div{class:"exercise-explain",span{class:"eyebrow","Explain the repair · self-review"}p{class:"small muted","Explain why AND expresses the requirement and why a cross-build case is necessary. The engine checks the four cases; it does not grade your prose."}
                textarea{class:"interactive-only","aria-label":"Explain your repair","data-testid":"repair-explanation",value:"{s.explanation}",placeholder:"Both approvals must refer to the same build because…",oninput:move|e|state.write().explanation=e.value()}
                p{class:"narrow-only","{s.explanation}"}
                button{class:"button primary interactive-only",style:"margin-top:14px","data-testid":"check-repair",disabled:s.busy,onclick:move |_|{
                    let mut s=state.write();let current=s.checks_revision==Some(s.policy_revision)&&s.policy==s.policy_draft;
                    if !current||s.checks.len()!=4{s.exercise_feedback="Run all four cases on the current policy first.".into();}
                    else if !s.checks.iter().all(|c|c.passed){s.exercise_feedback="Some cases still differ. Inspect the failing cases and revise the policy.".into();}
                    else if s.explanation.trim().is_empty(){s.exercise_feedback="Add your explanation and compare it with the authored reasoning before marking the exercise checked.".into();}
                    else{s.progress.steps[6].checked=true;s.exercise_feedback="Behavior verified: all four cases match. Explanation recorded for self-review; it was not automatically graded.".into();}
                    drop(s);persist(state);
                },"Check repair"}
            }
        }
    }
}

#[component]
fn Drawer() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    use_effect(move || focus("drawer-heading"));
    let s = state.read();
    let tutor = s.drawer.as_deref() == Some("tutor");
    let reference = lesson::REFERENCES
        .iter()
        .find(|r| r.0 == s.reference)
        .unwrap_or(&lesson::REFERENCES[0]);
    rsx! {aside{class:"drawer",role:"region","aria-labelledby":"drawer-heading",onkeydown:move|e|{if e.key()==Key::Escape{if tutor{integration::cancel_tutor(state);}state.write().drawer=None;focus(if tutor{"tutor-trigger"}else{"reference-trigger"});}},
        div{class:"drawer-header",h2{id:"drawer-heading",tabindex:"-1",if tutor{"Optional tutor"}else{"Reference"}}button{class:"button square","aria-label":"Close side panel",onclick:move |_|{if tutor{integration::cancel_tutor(state);}state.write().drawer=None;focus(if tutor{"tutor-trigger"}else{"reference-trigger"});},"×"}}
        if tutor{integration::TutorPanel {}}
        else{div{class:"drawer-list",for (id,title,_) in lesson::REFERENCES{button{class:if s.reference==id{"active"}else{""},onclick:move |_|state.write().reference=id.into(),"{title}"}}}h3{"{reference.1}"}p{style:"margin-top:14px","{reference.2}"}button{class:"button small",onclick:move |_|{state.write().drawer=None;navigate(state,Screen::Reference);},"Open reference library →"}}
    }}
}

#[component]
fn Completion() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let count = s.progress.steps.iter().filter(|p| p.checked).count();
    rsx! {section{class:"completion",span{class:"hero-symbol","aria-hidden":"true","∴"}span{class:"eyebrow","Release-readiness investigation"}h1{if count==7{"You investigated the decision."}else{"Your investigation so far."}}p{"You have checked {count} of 7 learning steps. Navigation stays open: revisit any step, inspect the evidence and try another explanation."}
        ul{for(i,step)in lesson::STEPS.iter().enumerate(){li{span{class:if s.progress.steps[i].checked{"mint"}else{"muted"},if s.progress.steps[i].checked{"✓"}else{"○"}}button{class:"text-action",onclick:move |_|open_step(state,i+1,true),"{step.title}"}}}}
        div{class:"button-row",button{class:"button primary",onclick:move |_|navigate(state,Screen::Course),"Back to course →"}button{class:"button",onclick:move |_|state.write().complete=false,"Return to workspace"}}
        p{class:"small muted",style:"margin-top:22px","Your progress is stored on this device. Export it from Local data to keep a portable copy."}
    }}
}

#[component]
fn ResetDialog() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    use_effect(move || focus("reset-cancel"));
    let n = state.read().step();
    rsx! {div{class:"dialog-backdrop",onclick:move |_|{state.write().reset_dialog=false;focus("lesson-reset");},
        section{class:"dialog",role:"dialog","aria-modal":"true","aria-labelledby":"reset-title",onclick:move|e|e.stop_propagation(),onkeydown:move|e|{if e.key()==Key::Escape{state.write().reset_dialog=false;focus("lesson-reset");}else if e.key()==Key::Tab{e.prevent_default();spawn(async move{let _=document::eval("const next=document.activeElement?.id==='reset-cancel'?'reset-confirm':'reset-cancel';document.getElementById(next)?.focus();return true;").await;});}},
            h2{id:"reset-title","Reset this step?"}p{"This opens the step's authored setup in a fresh session. Your saved learning progress remains. Current assertions and the current draft are replaced; older runs keep their original input identities."}
            div{class:"button-row",button{class:"button",id:"reset-cancel",onclick:move |_|{state.write().reset_dialog=false;focus("lesson-reset");},"Keep working"}button{class:"button primary",id:"reset-confirm","data-testid":"confirm-reset",onclick:move |_|{state.write().reset_dialog=false;open_step(state,n,true);focus("lesson-reset");},"Reset step"}}
        }
    }}
}

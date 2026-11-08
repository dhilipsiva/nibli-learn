use crate::{lesson, state::*};
use dioxus::prelude::*;
use nibli_learn_model::*;

#[component]
pub fn Course() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    let checked = s.progress.steps.iter().filter(|p| p.checked).count();
    let step = s.step();
    rsx! {main{id:"main-content",class:"page",
        div{class:"page-title",div{span{class:"eyebrow","Learn by investigating"}h1{"Make reasoning inspectable."}p{"Build precise claims, follow evidence, and learn what a machine's answer actually establishes."}}span{class:"badge ok","Phase 1 · available now"}}
        section{class:"hero",span{class:"eyebrow","Your learning workspace"}h2{"A release. Two approvals. One decision."}p{"Predict what follows, run the real Nibli engine in your browser, inspect its work, then change the evidence. Finish by repairing a policy whose successful example hides a bug."}
            div{class:"button-row",button{class:"button primary","data-testid":"start-lesson",disabled:s.busy,onclick:move |_|{if state.peek().session.is_some(){navigate(state,Screen::Lesson);}else{open_step(state,step,false);}},if checked>0{"Continue investigation →"}else{"Start the investigation →"}}span{class:"small mono muted","7 steps · {checked}/7 checked · no account needed"}}
        }
        div{class:"course-layout",
            aside{class:"track-list","aria-label":"Learning tracks",
                div{class:"track-button active",strong{"Shared foundations"}span{"A common language for reasoning"}}
                div{class:"track-button",strong{"Build Nibli"}span{"Planned · engine internals and local labs"}}
                div{class:"track-button",strong{"Integrate Applications"}span{"Planned · applications and model boundaries"}}
                div{class:"track-button",strong{"Choose an Approach"}span{"Planned · compare architectures and tradeoffs"}}
            }
            section{"aria-labelledby":"foundations-heading",span{class:"eyebrow","Shared foundations"}h2{id:"foundations-heading","A foundation you can test."}
                for (i,(title,description)) in lesson::FOUNDATIONS.iter().enumerate(){article{class:"lesson-row",
                    span{class:"number","{i+1:02}"}div{class:"lesson-copy",h3{"{title}"}p{"{description}"}span{class:if i==4{"badge ok"}else{"badge"},if i==4{"Available · seven-step investigation"}else{"Planned"}}}
                    if i==4{button{class:"button small",disabled:s.busy,onclick:move |_|navigate(state,Screen::Lesson),"Open →"}}
                }}
            }
        }
        div{class:"page-grid",style:"margin-top:36px",section{class:"card",span{class:"eyebrow","Explore"}h2{"Try your own statement."}p{"The local sandbox uses the same accepted knowledge and actual engine. Changes there affect your current investigation; reset a lesson step to restore its authored setup."}button{class:"button interactive-only",disabled:s.busy,onclick:move |_|navigate(state,Screen::Sandbox),"Open sandbox →"}}
            section{class:"card",span{class:"eyebrow","Keep the distinctions clear"}h2{"Reference, close at hand."}p{"Read concise explanations of claims, assertion records, source labels, closed-world outcomes, execution and proof."}button{class:"button",onclick:move |_|navigate(state,Screen::Reference),"Open reference →"}}
        }
        PageFooter {}
    }}
}

#[component]
pub fn ReferencePage() -> Element {
    rsx! {main{id:"main-content",class:"page",div{class:"page-title",div{span{class:"eyebrow","Reference"}h1{"Small distinctions. Better reasoning."}p{"Use these explanations alongside the actual inputs, outcome and recorded execution."}}}
        div{class:"reference-cards",for(id,title,body)in lesson::REFERENCES{article{id:"reference-{id}",h2{"{title}"}p{"{body}"}}}}
        section{class:"card",style:"margin-top:24px",h2{"Four outcomes, separate boundaries"}p{"TRUE means the claim follows. FALSE means it does not follow under this closed-world profile. UNKNOWN means the engine reports the claim undecided. RESOURCE_EXCEEDED means the configured reasoning limit prevented completion. Errors, cancelled operations and admission refusals are separate from these outcomes."}}
        PageFooter{}
    }}
}

#[component]
pub fn DataPage() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let count = s.progress.steps.iter().filter(|p| p.checked).count();
    rsx! {main{id:"main-content",class:"page",div{class:"page-title",div{span{class:"eyebrow","Local data"}h1{"Your work stays with you."}p{"No account, analytics or model connection is needed. Progress and accepted knowledge are saved in this browser when storage is available."}}}
        if !s.storage_error.is_empty(){div{class:"notice error",role:"alert",style:"margin-bottom:24px","{s.storage_error}" button{class:"button small",style:"margin-top:12px",onclick:move |_|export_raw_storage(),"Export existing browser data ↓"}}}
        div{class:"page-grid",
            section{class:"card",span{class:"eyebrow","Learning progress"}h2{"{count} of 7 steps checked"}p{"A progress file contains your lesson position, predictions and checked steps. It contains no source statements or execution recordings."}
                button{class:"button","data-testid":"progress-export",onclick:move |_|{download("nibli-learning-progress.json",serde_json::to_string_pretty(&state.peek().progress).unwrap_or_default());state.write().export_message="Progress export prepared. Your browser manages the download.".into();},"Export progress ↓"}
            }
            section{class:"card",span{class:"eyebrow","Current project"}h2{"Accepted knowledge and settings"}p{"Export assertion records, source labels and the engine recovery image for this session. Source labels can contain personal information you entered; review a file before sharing it."}
                button{class:"button",disabled:s.session.is_none(),onclick:move |_|{if let Some(session)=state.peek().session.clone(){download("nibli-project.json",serde_json::to_string_pretty(&ProjectExport{schema:SCHEMA_VERSION,kind:"nibli-project".into(),session}).unwrap_or_default());}},"Export current project ↓"}
            }
            section{class:"card",span{class:"eyebrow","Import progress"}h2{"Preview before replacing"}p{"Paste the contents of a progress export. The schema and lesson version are checked before your current progress can change."}
                div{class:"field",label{r#for:"progress-import","Progress JSON"}textarea{id:"progress-import","data-testid":"progress-import",rows:"6",value:"{s.import_text}",placeholder:"Paste a nibli-learning-progress.json export",oninput:move |e|{let mut s=state.write();s.import_text=e.value();s.import_preview=None;s.import_error.clear();}}}
                button{class:"button","data-testid":"progress-validate",onclick:move |_|{let result=Progress::import(&state.peek().import_text);let mut s=state.write();match result{Ok(progress)=>{s.import_preview=Some(progress);s.import_error.clear();},Err(error)=>{s.import_preview=None;s.import_error=error;}}},"Validate and preview"}
                if !s.import_error.is_empty(){p{class:"notice error",role:"alert",style:"margin-top:16px","{s.import_error}"}}
                if let Some(preview)=&s.import_preview{div{class:"data-preview",strong{"Compatible progress file"}p{"Step {preview.current_step} · {preview.steps.iter().filter(|p|p.checked).count()} checked steps. Replacing progress keeps your current accepted knowledge unchanged."}
                    button{class:"button primary","data-testid":"progress-import-confirm",onclick:move |_|{let progress=state.peek().import_preview.clone();if let Some(progress)=progress{let mut s=state.write();s.progress=progress;s.import_preview=None;s.import_text.clear();s.export_message="Imported validated progress. Existing accepted knowledge was retained.".into();drop(s);persist(state);}},"Replace learning progress"}
                }}
            }
            section{class:"card",span{class:"eyebrow","Run records"}h2{"Keep the evidence with the answer"}p{"Completed runs remain available in history while this page is open. Export a proof record to keep its original query, revision, settings, engine outcome, proof and execution events together. Full recordings are not saved to browser storage."}
                button{class:"button",disabled:s.run().is_none(),onclick:move |_|{if let Some(run)=state.peek().run(){download("nibli-run-record.json",serde_json::to_string_pretty(run).unwrap_or_default());}},"Export selected run ↓"}
                p{class:"small",style:"margin-top:20px","Opening the same lesson link on another device starts from its authored setup. It cannot reconstruct a private local run from a URL."}
            }
        }
        if !s.export_message.is_empty(){p{class:"notice",role:"status",style:"margin-top:22px","{s.export_message}"}}
        section{class:"card",style:"margin-top:24px",h2{"Browser storage has limits"}p{"Private browsing, storage restrictions or clearing site data can remove local work. Exports are portable copies. An invalid import is rejected without replacing your progress. No API key is collected or stored in this phase."}}
        PageFooter{}
    }}
}

#[component]
pub fn PacksPage() -> Element {
    rsx! {main{id:"main-content",class:"page",div{class:"page-title",div{span{class:"eyebrow","Content packs"}h1{"A small first release."}p{"The release-readiness investigation is bundled with this app and works with the engine in your browser."}}}
        section{class:"card",span{class:"badge ok","Installed · version 1"}h2{style:"margin-top:18px","Investigating and revising a decision"}p{"Seven original learning steps, authored hints, four isolated policy checks and reference explanations."}p{class:"small muted","Additional foundations and track lessons will arrive in later phases. External content-pack import is not enabled in this release."}}
        PageFooter{}
    }}
}

#[component]
fn PageFooter() -> Element {
    let state = use_context::<Signal<AppState>>();
    rsx! {footer{class:"page-footer",span{"Nibli Learn · original interactive learning"}a{href:"#/course",onclick:move |_|navigate(state,Screen::Course),"Course"}a{href:"#/data",onclick:move |_|navigate(state,Screen::Data),"Local data"}a{href:"#/packs",onclick:move |_|navigate(state,Screen::Packs),"Content packs"}a{href:"https://github.com/dhilipsiva/nibli-learn",target:"_blank",rel:"noopener noreferrer","Source ↗"}}}
}

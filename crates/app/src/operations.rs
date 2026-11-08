use crate::state::*;
use dioxus::dioxus_core::spawn_forever;
use dioxus::prelude::*;
use nibli_learn_model::*;
use serde_json::Value;

fn failure(error: &WorkerError, stage: &str) -> OperationResult {
    error.as_operation_failure(stage)
}

pub fn execute_free(state: Signal<AppState>, operation: Operation) {
    execute(state, Some(operation), None, true);
}
pub fn execute_baseline(state: Signal<AppState>) {
    let operation = state
        .peek()
        .learning
        .activity()
        .and_then(|a| a.operation.clone());
    execute(state, operation, None, false);
}
pub fn execute_action(state: Signal<AppState>, index: usize) {
    if index != state.peek().learning.action_cursor {
        return;
    }
    let action = state
        .peek()
        .learning
        .activity()
        .and_then(|a| a.actions.get(index))
        .cloned();
    if let Some(action) = action {
        execute(state, action.operation.clone(), Some(action), false);
    }
}
fn execute(
    mut state: Signal<AppState>,
    operation: Option<Operation>,
    action: Option<ActivityAction>,
    free: bool,
) {
    let (client, serial, current, expected, profile, changes, scope, version, free) = {
        let mut s = state.write();
        if s.busy {
            return;
        }
        let Some(client) = s.client.clone() else {
            return;
        };
        let Some(session) = s.session.clone() else {
            return;
        };
        let expected = if free {
            None
        } else if let Some(a) = &action {
            a.expected.clone()
        } else {
            s.learning.activity().and_then(|a| a.expected.clone())
        };
        let changes = match action
            .as_ref()
            .map(|a| resolve_activity_changes(&a.changes, &session))
            .transpose()
        {
            Ok(v) => v.unwrap_or_default(),
            Err(message) => {
                s.learning.feedback = format!(
                    "This action needs its preceding source state: {message} Reset the activity to repeat its ordered setup."
                );
                return;
            }
        };
        s.serial += 1;
        s.busy = true;
        s.operation = "running".into();
        s.playing = false;
        s.playback_token += 1;
        s.diagnostic = None;
        s.refusal = None;
        s.learning.failure = None;
        if action
            .as_ref()
            .is_some_and(|a| a.scope == ActionScope::Hypothetical)
        {
            s.learning.candidate = None;
            s.learning.apply_dialog = false;
        }
        s.active_attempt = Some(AbortedAttempt {
            id: format!("attempt-{}", s.serial),
            claim: operation
                .as_ref()
                .map(|o| o.source().to_owned())
                .unwrap_or_else(|| "source change".into()),
            epoch: session.epoch.clone(),
            revision: session.revision.clone(),
            settings: session.settings.clone(),
            status: "running".into(),
        });
        (
            client,
            s.serial,
            s.learning.workspace.progress.current.clone(),
            expected,
            action.as_ref().and_then(|a| a.profile.clone()),
            changes,
            action.as_ref().map(|a| a.scope.clone()).unwrap_or_default(),
            s.learning.course.version,
            free,
        )
    };
    spawn_forever(async move {
        let mut observed: Option<OperationResult> = None;
        let mut actual_run: Option<OperationRun> = None;
        let mut failure_record = None;
        let result: Result<(), WorkerError> = async {
            if scope == ActionScope::Hypothetical {
                let base = state.peek().session.clone().unwrap();
                let reply = client
                    .request(Command::Hypothetical {
                        candidate_epoch: epoch(),
                        source_epoch: base.epoch,
                        source_revision: base.revision,
                        changes,
                        profile,
                        operation,
                        activity_id: Some(current.activity.clone()),
                        content_version: version,
                    })
                    .await?;
                if state.peek().serial != serial {
                    return Ok(());
                }
                state.write().learning.candidate = reply.hypothetical;
                return Ok(());
            }
            if let Some(profile) = profile {
                let reply = client.request(Command::SetProfile { profile }).await?;
                if state.peek().serial != serial {
                    return Ok(());
                }
                state.write().session = Some(reply.session);
            }
            if !changes.is_empty() {
                let reply = client.request(Command::Change { changes }).await?;
                if state.peek().serial != serial {
                    return Ok(());
                }
                let mut s = state.write();
                s.session = Some(reply.session);
                s.change_record = reply.change_record;
            }
            if let Some(operation) = operation {
                let reply = client
                    .request(Command::Execute {
                        operation,
                        activity_id: (!free).then(|| current.activity.clone()),
                        content_version: version,
                    })
                    .await?;
                if state.peek().serial != serial {
                    return Ok(());
                }
                state.write().session = Some(reply.session);
                actual_run = reply.operation;
                observed = actual_run.as_ref().map(|r| r.result.clone());
            }
            Ok(())
        }
        .await;
        if state.peek().serial != serial {
            return;
        }
        let mut s = state.write();
        s.busy = false;
        s.operation = "ready".into();
        if let Err(error) = result {
            if matches!(
                error.kind.as_str(),
                "operational" | "serialization" | "cancelled"
            ) {
                s.abort_active_attempt();
                s.operation = "error".into();
            } else {
                s.active_attempt = None;
            }
            failure_record = error.record.clone();
            observed = Some(failure(&error, "change"));
            s.learning.failure = observed.clone();
            s.show_change = true;
            s.change_record = error.record.clone();
            s.diagnostic = Some(error);
        } else {
            s.active_attempt = None;
        }
        let mut evidence = None;
        if let Some(run) = actual_run {
            let current_main = s
                .session
                .as_ref()
                .is_some_and(|session| run.is_current_main(session));
            if current_main && !free && run.activity_id.as_deref() == Some(&current.activity) {
                evidence = Some(LearningEvidence {
                    engine_revision: run.engine_revision.clone(),
                    content_version: run.content_version,
                    run_id: run.id.clone(),
                    epoch: run.epoch.clone(),
                    revision: run.revision.clone(),
                    action: action.as_ref().map(|a| a.id.clone()),
                    result_family: run.result.family().into(),
                    outcome: match &run.result {
                        OperationResult::Decision { outcome, .. } => Some(outcome.status.clone()),
                        _ => None,
                    },
                });
            }
            if let Some(decision) = run.decision() {
                let step = s.step();
                s.retain_run(SavedRun::new(decision, None, step));
            }
            s.learning.retain(run);
            s.pane = Pane::Knowledge;
            s.show_change = false;
            s.selected_event = 0;
        }
        if scope == ActionScope::Hypothetical {
            let candidate_run = s.learning.candidate.as_ref().and_then(|c| c.run.clone());
            let base_current = s
                .learning
                .candidate
                .as_ref()
                .zip(s.session.as_ref())
                .is_some_and(|(c, session)| {
                    c.source_epoch == session.epoch && c.source_revision == session.revision
                });
            let matches = base_current
                && candidate_run
                    .as_ref()
                    .zip(expected.as_ref())
                    .is_some_and(|(run, expected)| {
                        matches!(run.scope, RunScope::Hypothetical { .. })
                            && run.activity_id.as_deref() == Some(&current.activity)
                            && expected.matches(&run.result)
                    });
            if matches {
                let run = candidate_run.unwrap();
                let progress = s.learning.progress_mut();
                progress.candidate_evidence.push(LearningEvidence {
                    engine_revision: run.engine_revision,
                    content_version: run.content_version,
                    run_id: run.id,
                    epoch: run.epoch,
                    revision: run.revision,
                    action: action.as_ref().map(|a| a.id.clone()),
                    result_family: run.result.family().into(),
                    outcome: match run.result {
                        OperationResult::Decision { outcome, .. } => Some(outcome.status),
                        _ => None,
                    },
                });
                if progress.candidate_evidence.len() > 64 {
                    progress.candidate_evidence.remove(0);
                }
                if let Some(index) = s.learning.activity().and_then(|a| {
                    a.actions
                        .iter()
                        .position(|item| action.as_ref().is_some_and(|action| action.id == item.id))
                }) {
                    s.learning.action_cursor = s.learning.action_cursor.max(index + 1);
                }
                s.operation = "candidate_complete".into();
                s.learning.feedback="The candidate matches its authored check. Its evidence is recorded separately and cannot satisfy a main-project check. Discard it or inspect the delta before applying.".into();
            } else {
                s.learning.feedback="The candidate did not complete its authored check. Inspect its actual result and source base; no hypothetical learning evidence was recorded.".into();
            }
        } else if !free {
            let matches = expected
                .as_ref()
                .zip(observed.as_ref())
                .is_some_and(|(expected, result)| expected.matches(result));
            let has_actual = evidence.is_some() || failure_record.is_some();
            if matches && has_actual {
                if failure_record.is_some()
                    && let Some(OperationResult::Failed {
                        stage, error_class, ..
                    }) = &observed
                    && let Some(session) = s.session.clone()
                {
                    let progress = s.learning.progress_mut();
                    progress.expected_failures.push(LearningFailureEvidence {
                        engine_revision: ENGINE_REVISION.into(),
                        content_version: version,
                        epoch: session.epoch,
                        revision: session.revision,
                        action: action.as_ref().map(|a| a.id.clone()),
                        stage: stage.clone(),
                        error_class: error_class.clone(),
                    });
                    if progress.expected_failures.len() > 64 {
                        progress.expected_failures.remove(0);
                    }
                }
                let progress = s.learning.progress_mut();
                if let Some(e) = evidence {
                    progress.evidence.push(e);
                    if progress.evidence.len() > 64 {
                        progress.evidence.remove(0);
                    }
                }
                if let Some(action) = &action {
                    progress.actions.insert(action.id.clone());
                    s.learning.action_cursor += 1;
                } else {
                    s.learning.baseline_done = true;
                }
                s.learning.feedback="This operation matches the authored check. Inspect the actual evidence and continue the diagnosis.".into();
            } else if observed.is_some() {
                s.learning.feedback="The result differs from the authored check. Read the actual operation result and current inputs. Reset to recover the activity’s original sequence if needed.".into();
            }
        }
        drop(s);
        persist(state);
    });
}
pub fn check_activity(mut state: Signal<AppState>) {
    let mut s = state.write();
    let Some(a) = s.learning.activity().cloned() else {
        return;
    };
    let p = s.learning.progress();
    let baseline = a.operation.is_none()
        || p.evidence
            .iter()
            .any(|e| e.action.is_none() && e.content_version == s.learning.course.version)
        || p.expected_failures
            .iter()
            .any(|e| e.action.is_none() && e.content_version == s.learning.course.version);
    let actions = a.actions.iter().all(|action| {
        if action.scope == ActionScope::Hypothetical {
            p.candidate_evidence.iter().any(|e| {
                e.action.as_deref() == Some(&action.id)
                    && e.content_version == s.learning.course.version
            })
        } else {
            p.actions.contains(&action.id)
        }
    });
    let quiz = a
        .question
        .as_ref()
        .is_none_or(|q| p.answer == Some(q.correct));
    let review = a.self_review.is_empty() || p.self_reviewed;
    let review_events = a.review_exercise.as_ref().is_none_or(|exercise| {
        exercise.milestones.iter().all(|milestone| {
            p.reviews.iter().any(|e| {
                &e.event == milestone
                    && e.source_id == exercise.draft.source_id
                    && e.engine_revision == ENGINE_REVISION
            })
        })
    });
    let lab = a.local_lab.as_ref().is_none_or(|id| {
        s.learning
            .labs
            .get(id)
            .zip(s.learning.workspace.lab_reports.get(id))
            .is_some_and(|(definition, report)| report.meets(definition))
    });
    if baseline && actions && quiz && review && review_events && lab {
        s.learning.progress_mut().checked = true;
        s.learning.feedback = if a.operation.is_none() {
            "Self-review recorded. This reflection is your assessment; it is not an engine or model grade.".into()
        } else {
            "Activity checked using recorded main-project evidence and your authored self-review."
                .into()
        };
    } else {
        s.learning.feedback = format!(
            "Still needed: {}{}{}{}{}{}",
            if !review_events {
                "complete the source review milestones; "
            } else {
                ""
            },
            if !lab {
                "attach a compatible local lab report and inspect it; "
            } else {
                ""
            },
            if !baseline {
                "run and inspect the initial operation; "
            } else {
                ""
            },
            if !actions {
                "complete the main-project actions in order; "
            } else {
                ""
            },
            if !quiz {
                "answer the knowledge question; "
            } else {
                ""
            },
            if !review {
                "complete the self-review."
            } else {
                ""
            }
        );
    }
    drop(s);
    persist(state);
}

#[component]
pub fn OperationToolbar() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    rsx! {div{class:"workspace-toolbar",h2{"Reasoning workspace"}div{class:"button-row",button{class:"button small",onclick:move |_|navigate(state,Screen::Data),"Projects"}if s.busy{button{class:"button small","data-testid":"run-cancel",onclick:move |_|cancel(state),"Cancel & restore"}}}div{class:"tabs",role:"tablist","aria-label":"Operation view",onkeydown:move|e|{
        let current=match state.peek().pane{Pane::Knowledge=>0,Pane::Replay=>1,Pane::Proof=>2};let next=match e.key(){Key::ArrowRight=>Some((current+1)%3),Key::ArrowLeft=>Some((current+2)%3),Key::Home=>Some(0),Key::End=>Some(2),_=>None};if let Some(next)=next{e.prevent_default();let mut s=state.write();s.pane=[Pane::Knowledge,Pane::Replay,Pane::Proof][next];s.selected_event=0;drop(s);focus(["operation-result-tab","operation-replay-tab","operation-proof-tab"][next]);}
    },for (pane,label)in[(Pane::Knowledge,"Result"),(Pane::Replay,"Execution replay"),(Pane::Proof,"Proof")]{button{id:match pane{Pane::Knowledge=>"operation-result-tab",Pane::Replay=>"operation-replay-tab",Pane::Proof=>"operation-proof-tab"},tabindex:if s.pane==pane{"0"}else{"-1"},role:"tab","aria-controls":match pane{Pane::Knowledge=>"operation-result-panel",Pane::Replay=>"operation-replay-panel",Pane::Proof=>"operation-proof-panel"},"aria-selected":s.pane==pane,class:if s.pane==pane{"active"}else{""},onclick:move |_|{state.write().pane=pane;state.write().selected_event=0;},"{label}"}}}}}
}

#[component]
pub fn ActivityControls() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let Some(a) = s.learning.activity().cloned() else {
        return rsx! {};
    };
    let p = s.learning.progress();
    rsx! {section{class:"operation-controls interactive-only",if let Some(op)=&a.operation{label{"Your prediction or diagnosis" input{value:p.prediction.clone().unwrap_or_default(),maxlength:"256",placeholder:"What do you expect, and why?",oninput:move|e|{state.write().learning.progress_mut().prediction=Some(e.value());persist(state);}}}pre{"{op.source()}"}button{class:"button primary","data-testid":"activity-run",disabled:s.busy,onclick:move |_|execute_baseline(state),"Run initial operation"}}
        if !a.actions.is_empty(){h3{"Investigate and repair"}p{class:"small muted","Actions use the preceding state. A refusal is a result to diagnose; reset this activity to repeat the original sequence."}for(i,action)in a.actions.iter().enumerate(){div{class:if i==s.learning.action_cursor{"authored-action current"}else{"authored-action"},h4{"{i+1}. {action.title}"}p{"{action.instruction}"}if action.scope==ActionScope::Hypothetical{span{class:"badge","Detached candidate"}}button{class:"button small","data-testid":"action-{action.id}",disabled:s.busy||i!=s.learning.action_cursor,title:if i>s.learning.action_cursor{"Complete the preceding authored action first."}else if i<s.learning.action_cursor{"Already attempted in this sequence. Reset to repeat."}else{"Run this authored action."},onclick:move |_|execute_action(state,i),if i<s.learning.action_cursor{"✓ Attempt recorded"}else{"Run this action"}}}}}
    }}
}

#[component]
pub fn OperationResultView() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let selected = if s.learning.failure.is_some() {
        None
    } else {
        s.learning.selected().cloned()
    };
    let result = s
        .learning
        .failure
        .clone()
        .or_else(|| selected.as_ref().map(|r| r.result.clone()));
    let stale = selected
        .as_ref()
        .zip(s.session.as_ref())
        .is_some_and(|(r, session)| !r.is_current_main(session));
    rsx! {section{class:"operation-results",id:match s.pane{Pane::Knowledge=>"operation-result-panel",Pane::Replay=>"operation-replay-panel",Pane::Proof=>"operation-proof-panel"},role:if s.screen==Screen::Lesson{"tabpanel"}else{"region"},"aria-labelledby":if s.screen==Screen::Lesson{Some(match s.pane{Pane::Knowledge=>"operation-result-tab",Pane::Replay=>"operation-replay-tab",Pane::Proof=>"operation-proof-tab"})}else{None},"aria-label":"Actual operation result",
        if !s.learning.operation_runs.is_empty(){div{class:"history-scroll","aria-label":"Operation history",for(i,run)in s.learning.operation_runs.iter().enumerate(){button{class:if s.learning.selected_operation==Some(i){"chip active"}else{"chip"},title:"{run.id}",onclick:move |_|{let r=state.peek().learning.operation_runs[i].clone();let mut s=state.write();s.learning.selected_operation=Some(i);s.learning.failure=None;s.selected_event=0;if let Some(d)=r.decision(){let step=s.step();s.retain_run(SavedRun::new(d,None,step));}},"Run {i+1} · {run.result.family()}"}}}}
        if s.learning.failure.is_some() {p{class:"notice","Failed operation · no completed main-project run is attached to this refusal."}if s.change_record.is_some(){button{class:"button small",onclick:move |_|{if let Some(record)=&state.peek().change_record{download("nibli-failed-operation-record.json",serde_json::to_string_pretty(record).unwrap_or_default());}},"Export actual failure recording"}}}
        if let Some(run)=selected.clone(){div{class:"operation-meta",span{class:if stale{"badge warn"}else{"badge ok"},if stale{"Historical · inputs changed"}else{"Current main-project evidence"}}span{"Revision {run.revision}"}span{"Profile {run.profile.id} v{run.profile.version}"}button{class:"button small","data-testid":"operation-export",onclick:move |_|{if let Some(run)=state.peek().learning.selected(){download("nibli-operation.json",serde_json::to_string_pretty(run).unwrap_or_default());}},"Export record"}}details{summary{"Run identity and profile"}pre{{serde_json::to_string_pretty(&serde_json::json!({"id":run.id,"epoch":run.epoch,"revision":run.revision,"profile":run.profile,"engine_revision":run.engine_revision,"scope":run.scope})).unwrap_or_default()}}}}
        if s.pane==Pane::Replay {
            if s.execution_record().is_some_and(|r|!r.is_null()){div{class:"interactive-only",crate::inspectors::Replay {}}}
            else{div{class:"empty",h3{"No execution chronology is available"}p{"This inspection or failed operation did not supply a replayable recording. Its actual result remains available in Result."}}}
        }else if s.pane==Pane::Proof{
            if selected.as_ref().is_some_and(|r|r.proof.is_some()){div{class:"interactive-only",crate::inspectors::Proof {}}if let Some(proof)=selected.as_ref().and_then(|r|r.proof.as_ref()){pre{class:"narrow-only","{proof.text}"}}}
            else{div{class:"empty",h3{"This operation has no proof certificate"}p{"Answers, counts, scans and compiler inspections expose their actual result and any recorded execution. No Boolean proof is invented for them."}}}
        }else if let Some(result)=result{TypedResult{result_json:serde_json::to_string(&result).unwrap_or_default()}if matches!(result,OperationResult::Compiled{..}){if let Some(record)=selected.as_ref().map(|r|&r.record){details{summary{"Recorded compiler pipeline · source, AST debug arrays and lowered IR"}pre{{serde_json::to_string_pretty(record.get("artifacts").unwrap_or(&Value::Null)).unwrap_or_default()}}}}}}
        else if s.screen==Screen::Lesson && s.learning.activity().is_some_and(|a|a.operation.is_none()){
            p{class:"notice","Explore the activity below and record your reasoning in Self-review. This activity does not require an initial engine query."}
        }
        else{div{class:"empty",span{class:"empty-symbol","∴"}h3{"Make a prediction, then investigate."}p{"Run the supplied operation to see the engine’s actual result here."}}}
    }}
}

#[component]
fn TypedResult(result_json: String) -> Element {
    let Ok(result) = serde_json::from_str::<OperationResult>(&result_json) else {
        return rsx! {p{"Result could not be displayed."}};
    };
    let family = result.family();
    rsx! {div{class:"typed-result","data-testid":"operation-result","data-family":family,
        match result{
            OperationResult::Decision{outcome,why}=>rsx!{span{class:"eyebrow","Engine decision"}strong{class:"result-value","data-status":"{outcome.status}","{outcome.status}"}p{"{why}"}if let Some(detail)=outcome.detail{p{class:"badge","Reason / resource: {detail}"}}if outcome.naf_dependent{p{class:"notice","This result depends on negation as failure."}}},
            OperationResult::Answers{rows,count}=>rsx!{span{class:"eyebrow","Complete answer collection"}h3{"{count} answer bindings"}if count=="0"{p{"The completed query has no answers in the represented domain."}}else{AnswerRows{rows_json:rows.to_string()}}},
            OperationResult::Counted{count}=>rsx!{span{class:"eyebrow","Complete distinct-binding count"}strong{class:"result-value","{count}"}p{"This counts query bindings, not source records."}},
            OperationResult::Aggregated{value,witnesses}=>rsx!{span{class:"eyebrow","Aggregate result"}strong{class:"result-value",if let Some(value)=value{"{value}"}else{"Empty"}}p{"{witnesses} witnesses in the completed collection."}if value.is_none(){p{"Empty has not been converted to zero."}}},
            OperationResult::Contradictions{violations,unresolved,completed}=>rsx!{h3{if completed&&violations.is_empty(){"Clean completed scan"}else if completed{"Completed scan · violations found"}else{"Scan has unresolved checks"}}p{"Checks concern the represented constraints."}h4{"Violations ({violations.len()})"}ul{for v in violations{li{"{v}"}}}h4{"Unresolved checks"}pre{"{serde_json::to_string_pretty(&unresolved).unwrap_or_default()}"}},
            OperationResult::Compiled{logic,structural_preflight}=>rsx!{h3{"Source compiled"}p{"Compilation did not admit a statement or establish a truth value."}p{match structural_preflight{StructuralPreflight::NotRequested=>"Structural preflight not requested.".into(),StructuralPreflight::Passed=>"Structural preflight passed; actual admission remains a separate operation.".into(),StructuralPreflight::Refused{message}=>format!("Structural preflight refused: {message}")}}details{open:true,summary{"Actual compiler output"}pre{"{serde_json::to_string_pretty(&logic).unwrap_or_default()}"}}},
            OperationResult::Inspection{view,data}=>rsx!{h3{"Actual {view:?} inspection"}pre{"{serde_json::to_string_pretty(&data).unwrap_or_default()}"}},
            OperationResult::CollectionUnavailable{kind,message}=>rsx!{h3{"Collection unavailable"}span{class:"badge warn","{kind}"}p{"{message}"}p{"No exhaustive answer list, empty collection or total is claimed."}},
            OperationResult::Failed{stage,error_class,message}=>rsx!{h3{"Operation failed"}span{class:"badge warn","{stage} · {error_class}"}p{"{message}"}p{"This is not a FALSE decision."}},
        }
    }}
}
#[component]
fn AnswerRows(rows_json: String) -> Element {
    let rows: Value = serde_json::from_str(&rows_json).unwrap_or_default();
    rsx! {div{class:"answer-rows",for(i,row)in rows.as_array().into_iter().flatten().enumerate(){article{class:"answer-row",h4{"Answer {i+1}"}for binding in row.as_array().into_iter().flatten(){dl{dt{{text(binding,"variable")}}dd{code{{binding.get("term").map(|v|v.to_string()).unwrap_or_default()}}span{class:"badge",{format!("Origin: {}",text(binding,"origin"))}}}}}}}}}
}

#[component]
pub fn AcceptedSources() -> Element {
    let state = use_context::<Signal<AppState>>();
    let s = state.read();
    rsx! {
        details { class: "accepted-sources", open: true,
            summary { "Accepted source records" }
            if let Some(session) = &s.session {
                for a in session.assertions.iter().filter(|a| a.active) {
                    article { key: "{a.id}", id: "assertion-{a.id}", tabindex: "-1", class: "assertion-row", "data-assertion-id": "{a.id}",
                        span { class: "eyebrow", "Source · {a.source}" }
                        pre { "{a.text}" }
                        span { class: "small muted", "Assertion {a.id}" }
                        if let Some(review)=&a.review { details { summary { "Learner-reviewed source provenance" } p { "{review.source_id}: {review.source_text}" } p { class:"small muted", "Source hash {review.source_sha256}. This is retained learner attestation, not independent authentication." } } }
                        button { class: "button small interactive-only", disabled: s.busy,
                            onclick: { let id = a.id.clone(); move |_| send(state, Command::Change { changes: vec![Change::Withdraw { id: id.clone() }] }, Purpose::Change) },
                            "Withdraw this source"
                        }
                    }
                }
            } else {
                p { "No project is initialized." }
            }
            SourceEditor {}
        }
    }
}

#[component]
pub fn CandidateView() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let Some(candidate) = s.learning.candidate.clone() else {
        return rsx! {};
    };
    let stale = s.session.as_ref().is_none_or(|v| {
        v.epoch != candidate.source_epoch || v.revision != candidate.source_revision
    });
    rsx! {section{class:"candidate-panel",h2{"Detached candidate"}span{class:"badge warn",if stale{"Base changed · retest before applying"}else{"Separate from the main project"}}p{"Source revision {candidate.source_revision}; candidate epoch {candidate.candidate.epoch}."}details{summary{"Candidate profile and proposed delta"}pre{{serde_json::to_string_pretty(&serde_json::json!({"profile":candidate.profile,"changes":candidate.proposed_delta})).unwrap_or_default()}}}if let Some(run)=candidate.run{TypedResult{result_json:serde_json::to_string(&run.result).unwrap_or_default()}details{summary{"Actual candidate recording and proof"}pre{"{serde_json::to_string_pretty(&run).unwrap_or_default()}"}}}div{class:"button-row interactive-only",button{class:"button",onclick:move |_|state.write().learning.candidate=None,"Discard candidate"}button{class:"button",disabled:s.busy,onclick:move |_|retest_candidate(state),"Retest against current main inputs"}button{class:"button primary",id:"candidate-apply-trigger",disabled:s.busy||stale,onclick:move |_|state.write().learning.apply_dialog=true,"Preview Apply as real change"}}}}
}

#[component]
pub fn ApplyDialog() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let Some(c) = s.learning.candidate.clone() else {
        return rsx! {};
    };
    use_effect(move || focus("apply-cancel"));
    rsx! {div{class:"modal-backdrop",div{class:"modal",role:"dialog","aria-modal":"true","aria-labelledby":"apply-heading",onkeydown:move|e|{if e.key()==Key::Escape{state.write().learning.apply_dialog=false;focus("candidate-apply-trigger");}else if e.key()==Key::Tab{e.prevent_default();let shift=e.modifiers().shift();spawn(async move{let code=format!("const dialog=document.querySelector(\"[aria-labelledby=apply-heading]\");const nodes=[...dialog.querySelectorAll(\"button:not(:disabled), summary\")];const index=nodes.indexOf(document.activeElement);nodes[(index+({}?-1:1)+nodes.length)%nodes.length]?.focus();return true;",shift);let _=document::eval(&code).await;});}},h2{id:"apply-heading",tabindex:"-1","Apply these source changes?"}p{"This performs fresh atomic admission against the recorded main-project base. Candidate IDs and profile are not copied. The main query is rerun under the current main profile."}pre{"{serde_json::to_string_pretty(&c.proposed_delta).unwrap_or_default()}"}p{"Base epoch {c.source_epoch}, revision {c.source_revision}."}details{summary{"Current main profile"}pre{"{s.session.as_ref().and_then(|v|v.profile.as_ref()).and_then(|v|serde_json::to_string_pretty(v).ok()).unwrap_or_default()}"}}div{class:"button-row",button{class:"button",id:"apply-cancel",onclick:move |_|{state.write().learning.apply_dialog=false;focus("candidate-apply-trigger");},"Keep separate"}button{class:"button primary",id:"apply-confirm",disabled:s.busy,onclick:move |_|apply_candidate(state),"Confirm Apply to main project"}}}}}
}
fn apply_candidate(mut state: Signal<AppState>) {
    let (client, candidate, serial) = {
        let mut s = state.write();
        if s.busy {
            return;
        }
        let (Some(client), Some(candidate)) = (s.client.clone(), s.learning.candidate.clone())
        else {
            return;
        };
        s.busy = true;
        s.serial += 1;
        (client, candidate, s.serial)
    };
    spawn_forever(async move {
        let result = client
            .request(Command::ApplyDelta {
                source_epoch: candidate.source_epoch,
                source_revision: candidate.source_revision,
                changes: candidate.proposed_delta,
            })
            .await;
        if state.peek().serial != serial {
            return;
        }
        let operation = candidate.run.map(|r| r.operation);
        {
            let mut s = state.write();
            s.busy = false;
            s.learning.apply_dialog = false;
            match result {
                Ok(reply) => {
                    s.session = Some(reply.session);
                    s.change_record = reply.change_record;
                    s.learning.candidate = None;
                    s.notice="Candidate delta admitted with fresh main-project identities. Running the main query under its current profile.".into();
                }
                Err(error) => {
                    s.notice = format!(
                        "Apply refused; the main project was retained: {}",
                        error.message
                    );
                    return;
                }
            }
        }
        persist(state);
        focus("main-content");
        if let Some(operation) = operation {
            execute_free(state, operation);
        }
    });
}

pub fn announce(result: &OperationResult) -> String {
    match result {
        OperationResult::Decision { outcome, .. } => {
            format!("Decision complete: {}.", outcome.status)
        }
        OperationResult::Answers { count, .. } => {
            format!("Complete answer collection: {count} bindings.")
        }
        OperationResult::Counted { count } => format!("Complete count: {count}."),
        OperationResult::Aggregated { value, .. } => value
            .map(|v| format!("Aggregate complete: {v}."))
            .unwrap_or_else(|| "Aggregate complete: Empty.".into()),
        OperationResult::Contradictions {
            completed,
            violations,
            ..
        } => format!(
            "Constraint scan {}. {} violations reported.",
            if *completed {
                "completed"
            } else {
                "has unresolved checks"
            },
            violations.len()
        ),
        OperationResult::Compiled { .. } => "Source compiled. No knowledge was admitted.".into(),
        OperationResult::Inspection { .. } => {
            "Inspection complete. No execution chronology is provided for this inspection.".into()
        }
        OperationResult::CollectionUnavailable { kind, .. } => {
            format!("Collection unavailable: {kind}. No complete total is claimed.")
        }
        OperationResult::Failed {
            stage, error_class, ..
        } => format!("Operation failed at {stage}: {error_class}. This is not a Boolean decision."),
    }
}

fn retest_candidate(state: Signal<AppState>) {
    let action = state
        .peek()
        .learning
        .activity()
        .and_then(|a| {
            a.actions
                .iter()
                .find(|a| a.scope == ActionScope::Hypothetical)
        })
        .cloned();
    if let Some(action) = action {
        execute(state, action.operation.clone(), Some(action), false);
    }
}

#[component]
fn SourceEditor() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    rsx! {details{class:"source-editor interactive-only",summary{"Try your own source change"}p{"These are real main-project changes. They can stale earlier results and candidate bases; they do not automatically satisfy an authored learning check."}label{"Statement source" textarea{rows:"4",value:"{s.draft}",oninput:move|e|state.write().draft=e.value(),spellcheck:"false"}}label{"Source label" input{value:"{s.source}",oninput:move|e|state.write().source=e.value(),placeholder:"Where this proposed assertion comes from"}}button{class:"button primary",disabled:s.busy||s.draft.trim().is_empty()||s.source.trim().is_empty(),onclick:move |_|{let text=state.peek().draft.clone();let source=state.peek().source.clone();send(state,Command::Change{changes:vec![Change::Assert{text,source}]},Purpose::Change);},"Admit this source"}if let Some(error)=&s.diagnostic{p{class:"notice error",role:"alert","{error.message}"}}label{"Maximum chain depth (1–256)" input{r#type:"number",min:"1",max:"256",value:"{s.settings().depth}",disabled:s.busy,onchange:move|e|{if let Ok(depth)=e.value().parse::<u32>() && (1..=256).contains(&depth){let mut profile=state.peek().session.as_ref().and_then(|v|v.profile.clone()).unwrap_or_default();profile.id="learner.profile".into();profile.max_chain_depth=depth;send(state,Command::SetProfile{profile},Purpose::Settings);}}}}p{class:"small muted","Changing the input profile makes previous evidence historical. Candidate profiles remain separate."}}}
}

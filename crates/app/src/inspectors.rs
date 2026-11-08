use crate::state::*;
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[component]
pub fn Replay() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let events = s.events();
    let total = events.len();
    let selected = s.selected_event.min(total.saturating_sub(1));
    let search = s.event_search.to_lowercase();
    let filtered: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            let matches_filter = match s.event_filter.as_str() {
                "failed" => event_failed(event),
                "proof" => event_proof_node(event).is_some(),
                _ => true,
            };
            matches_filter
                && (search.is_empty() || event.to_string().to_lowercase().contains(&search))
        })
        .map(|(i, _)| i)
        .collect();
    let position = filtered.iter().position(|i| *i == selected).unwrap_or(0);
    let window_start = position.saturating_sub(20);
    let earlier = filtered
        .get(window_start.saturating_sub(40))
        .copied()
        .unwrap_or(0);
    let visible: Vec<usize> = filtered
        .iter()
        .skip(window_start)
        .take(60)
        .copied()
        .collect();
    let parents: HashMap<String, String> = events
        .iter()
        .filter_map(|e| {
            let frame = text(e, "frame_id");
            let parent = text(e, "parent_frame_id");
            if frame.is_empty() || parent.is_empty() || parent == "null" {
                None
            } else {
                Some((frame, parent))
            }
        })
        .collect();
    let has_query = s.run().is_some();
    let recording = if s.show_change {
        s.change_record
            .as_ref()
            .map(|r| text(r, "status"))
            .unwrap_or_default()
    } else {
        s.run()
            .map(|r| r.recording_status().to_owned())
            .unwrap_or_default()
    };
    rsx! {
        div { class:"replay-header",
            div { h2 { "Execution replay" } p { "Recorded operations · playback never reruns reasoning" } }
            div { class:"segmented",
                button { disabled:!has_query, "aria-pressed":!s.show_change, onclick:move |_|{ let mut s=state.write();s.show_change=false;s.selected_event=0;s.playing=false;s.playback_token+=1; }, "Query" }
                button { disabled:s.change_record.is_none(), "aria-pressed":s.show_change, onclick:move |_|{ let mut s=state.write();s.show_change=true;s.selected_event=0;s.playing=false;s.playback_token+=1; }, "Last change" }
            }
        }
        if total==0 {
            div { class:"empty", span{class:"empty-symbol","aria-hidden":"true","↳"} h2 { "No recording selected" }
                p { "Run a claim or make a knowledge change, then inspect the operations recorded by the engine. An exported record contains its original inputs and outcome." }
                button { class:"button", onclick:move |_|open_pane(state,Pane::Knowledge), "Return to Knowledge" }
            }
        } else {
            div { class:"button-row",span { class:if recording=="complete" {"badge ok"} else {"badge warn"}, "Recording · {recording}" }
                span { class:"small muted", "{total} recorded events" }
                button { class:"text-action", onclick:move |_| { let record=if state.peek().show_change {state.peek().change_record.clone()}else{state.peek().run().map(|r|r.record.clone())};if let Some(record)=record {download("nibli-execution-record.json",serde_json::to_string_pretty(&record).unwrap_or_default());} }, "Export record ↓" }
            }
            if recording!="complete" { p {class:"notice",style:"margin-top:14px","The recording is {recording}. The available events may not cover the entire operation. Recording completeness is separate from the query outcome."} }
            div { class:"playback interactive-only", "aria-label":"Playback controls",
                button { class:"button square", "aria-label":"First recorded event", disabled:selected==0, onclick:move |_|seek(state,0), "|←" }
                button { class:"button square", "data-testid":"replay-prev", "aria-label":"Previous recorded event", disabled:selected==0, onclick:move |_|seek(state,selected.saturating_sub(1)), "←" }
                button { class:"button", "data-testid":"replay-play", onclick:move |_|play(state), if s.playing {"Pause"} else {"Play"} }
                button { class:"button square", "data-testid":"replay-next", "aria-label":"Next recorded event", disabled:selected+1>=total, onclick:move |_|seek(state,(selected+1).min(total-1)), "→" }
                button { class:"button square", "aria-label":"Last recorded event", disabled:selected+1>=total, onclick:move |_|seek(state,total-1), "→|" }
                input { r#type:"range", min:"0", max:"{total-1}", value:"{selected}", "aria-label":"Recorded event position", oninput:move|e|{if let Ok(position)=e.value().parse(){seek(state,position);}} }
                span { class:"position", "data-testid":"replay-position", "{selected+1} / {total}" }
            }
            div { class:"filter-bar",
                label { r#for:"event-filter", "Show" }
                select { id:"event-filter", value:"{s.event_filter}", onchange:move |e|state.write().event_filter=e.value(),
                    option {value:"all","All events"} option {value:"failed","Failed attempts"} option {value:"proof","Linked to proof"}
                }
                input { r#type:"search", "aria-label":"Search recorded events", placeholder:"Search this recording…", value:"{s.event_search}", oninput:move|e|state.write().event_search=e.value() }
                span { class:"small muted", "{filtered.len()} matching" }
            }
            div { class:"replay-layout",
                div { class:"events", "aria-label":"Recorded operations",
                    if filtered.is_empty() { p {class:"notice","No recorded events match this filter. Clear the filter to inspect the full recording."} }
                    if window_start>0 {button{class:"text-action",onclick:move |_|seek(state,earlier),"↑ Earlier matching events"}}
                    for index in visible {
                        { let event=&events[index]; let title=event_title(event); let kind=text(event,"kind").replace('_'," "); let depth=frame_depth(event,&parents).min(5); let failed=event_failed(event);
                        rsx! { button { key:"{index}", class:format!("event{}{}",if index==selected{" selected"}else{""},if failed{" failed"}else{""}), style:"margin-left:{depth*8}px", "aria-pressed":index==selected, onclick:move |_|seek(state,index),
                            span {class:"event-index","{index+1}"} span {class:"event-glyph","aria-hidden":"true",if failed{"×"}else{"↳"}}
                            span {span{class:"event-kind","{kind}"}span{class:"event-summary","{title}"}}
                        }} }
                    }
                    if window_start+60<filtered.len() {button{class:"text-action",onclick:move |_|{let len=state.peek().events().len();seek(state,(selected+40).min(len.saturating_sub(1)));},"↓ Later events"}}
                    p {class:"small muted",style:"margin:14px 8px","The list shows a window around your selection. Playback and the position slider address the complete saved record."}
                }
                Snapshot {}
            }
        }
    }
}

fn frame_depth(event: &Value, parents: &HashMap<String, String>) -> usize {
    let mut current = text(event, "frame_id");
    let mut seen = HashSet::new();
    let mut depth = 0;
    while let Some(parent) = parents.get(&current) {
        if !seen.insert(current.clone()) {
            break;
        }
        depth += 1;
        current = parent.clone();
        if depth > 50 {
            break;
        }
    }
    depth
}
fn seek(mut state: Signal<AppState>, position: usize) {
    let mut s = state.write();
    s.selected_event = position;
    s.playing = false;
    s.playback_token += 1;
}
fn play(mut state: Signal<AppState>) {
    let token = {
        let mut s = state.write();
        s.playback_token += 1;
        s.playing = !s.playing;
        if !s.playing {
            return;
        }
        if s.selected_event + 1 >= s.events().len() {
            s.selected_event = 0;
        }
        s.playback_token
    };
    spawn(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(550).await;
            let mut s = state.write();
            if s.playback_token != token || !s.playing {
                return;
            }
            let total = s.events().len();
            if s.selected_event + 1 >= total {
                s.playing = false;
                return;
            }
            s.selected_event += 1;
        }
    });
}

#[component]
fn Snapshot() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let Some(event) = s.selected_event_value() else {
        return rsx! {aside{class:"snapshot",h3{"Select an event"}}};
    };
    let snapshot = event.get("state").cloned().unwrap_or(Value::Null);
    let proof_node = event_proof_node(&event);
    let assertions: Vec<String> = event
        .get("assertion_ids")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    let can_link = s
        .run()
        .zip(s.session.as_ref())
        .is_some_and(|(r, session)| r.epoch == session.epoch);
    let artifact_id = event
        .get("source")
        .map(|source| text(source, "artifact_id"))
        .unwrap_or_default();
    let record = if s.show_change {
        s.change_record.as_ref()
    } else {
        s.run().map(|r| &r.record)
    };
    let source_text = record
        .and_then(|r| r.get("artifacts"))
        .and_then(Value::as_array)
        .and_then(|artifacts| artifacts.iter().find(|a| text(a, "id") == artifact_id))
        .map(|a| text(a, "source"));
    rsx! {aside{class:"snapshot","aria-label":"Recorded state snapshot",
        span{class:"eyebrow","Snapshot · event {s.selected_event+1}"}h3{"{event_title(&event)}"}
        dl{for (key,label) in [("goal","Goal"),("depth","Depth"),("bindings","Bindings"),("candidate","Candidate"),("rule","Rule"),("memo","Memo"),("memo_scope","Memo scope")]{
            dt{"{label}"}dd{if let Some(value)=snapshot.get(key).filter(|v|!v.is_null()){ "{value.as_str().map(str::to_owned).unwrap_or_else(||value.to_string())}" }else{span{class:"muted","Not recorded"}}}
        }dt{"Result"}dd{ {text(&event,"result")} }dt{"Frame"}dd{ {text(&event,"frame_id")} }dt{"Parent frame"}dd{ {text(&event,"parent_frame_id")} }}
        if let Some(node)=proof_node{button{class:"button small",onclick:move |_|{let mut s=state.write();s.selected_node=Some(node);s.pane=Pane::Proof;s.proof_raw=true;s.proof_text=false;s.viewed_proof=true;s.show_change=false;},"Show proof node #{node} →"}}
        if !assertions.is_empty(){div{style:"margin-top:14px",span{class:"eyebrow","Assertion records"}for id in assertions{
            button{class:"text-action",style:"margin-right:12px",disabled:!can_link,onclick:move |_|{state.write().linked_assertion=Some(id.clone());open_pane(state,Pane::Knowledge);focus(&format!("assertion-{id}"));},"#{id}"}
        }}if !can_link{p{class:"small muted","These identifiers belong to the saved run's session, not the currently open session."}}}
        if let Some(source)=source_text{details{summary{"Recorded source text"}pre{"{source}"}}}
        details{summary{"Recorded source location"}pre{ {serde_json::to_string_pretty(event.get("source").unwrap_or(&Value::Null)).unwrap_or_default()} }}
        details{summary{"Complete event JSON"}pre{"{serde_json::to_string_pretty(&event).unwrap_or_default()}"}}
    }}
}

#[derive(Clone)]
struct GraphRow {
    id: usize,
    depth: usize,
    reference: bool,
}
fn graph_rows(steps: &[Value], root: usize) -> Vec<GraphRow> {
    let mut rows = vec![];
    let mut stack = vec![(root, 0)];
    let mut seen = HashSet::new();
    while let Some((id, depth)) = stack.pop() {
        if id >= steps.len() || rows.len() >= 256 {
            continue;
        }
        let reference = !seen.insert(id);
        rows.push(GraphRow {
            id,
            depth: depth.min(12),
            reference,
        });
        if !reference && let Some(children) = steps[id].get("children").and_then(Value::as_array) {
            for child in children.iter().rev() {
                if let Some(child) = child.as_u64() {
                    stack.push((child as usize, depth + 1));
                }
            }
        }
    }
    rows
}
fn node_label(step: &Value) -> String {
    let rule = step.get("rule").unwrap_or(&Value::Null);
    for key in [
        "fact",
        "detail",
        "failed_condition",
        "label",
        "predicate",
        "original",
    ] {
        let value = text(rule, key);
        if !value.is_empty() {
            return value;
        }
    }
    text(rule, "type").replace('_', " ")
}
fn node_sources(step: &Value) -> Vec<String> {
    step.get("rule")
        .and_then(|r| r.get("sources"))
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|s| s.get("id").or_else(|| s.get("assertion_id")))
                .map(|s| {
                    s.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

#[component]
pub fn Proof() -> Element {
    let mut state = use_context::<Signal<AppState>>();
    let s = state.read();
    let Some(run) = s.run() else {
        return rsx! {div{class:"empty",span{class:"empty-symbol","aria-hidden":"true","∴"}h2{"No proof selected"}p{"Run a claim, then inspect the proof returned by the engine. An operational error or admission refusal has no Boolean proof outcome."}button{class:"button",onclick:move |_|open_pane(state,Pane::Knowledge),"Return to Knowledge"}}};
    };
    let steps = run
        .proof
        .get("steps")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let root = run.proof.get("root").and_then(Value::as_u64).unwrap_or(0) as usize;
    let rows = graph_rows(steps, root);
    let node = s.selected_node.unwrap_or(root);
    let selected = steps.get(node);
    let can_link = s
        .session
        .as_ref()
        .is_some_and(|session| session.epoch == run.epoch);
    let proof_text = run.proof_text.clone();
    rsx! {
        div{class:"replay-header",div{h2{"Proof inspection"}p{"A derivation from this run's inputs · {steps.len()} proof nodes"}}crate::Verdict{status:run.outcome.status.clone(),detail:run.outcome.detail.clone()}}
        p{class:"explanation-line","{run.why}"}
        div{class:"proof-toolbar interactive-only",
            div{class:"segmented",button{"aria-pressed":!s.proof_text&&!s.proof_raw,onclick:move |_|{let mut s=state.write();s.proof_text=false;s.proof_raw=false;},"Explanation"}button{"aria-pressed":!s.proof_text&&s.proof_raw,"data-testid":"raw-proof-toggle",onclick:move |_|{let mut s=state.write();s.proof_text=false;s.proof_raw=true;},"Raw graph"}button{"aria-pressed":s.proof_text,onclick:move |_|state.write().proof_text=true,"Text"}}
            button{class:"button small","data-testid":"proof-export",onclick:move |_|{if let Some(run)=state.peek().run(){download("nibli-proof-record.json",serde_json::to_string_pretty(&json!({"schema":1,"kind":"proof-record","run":run})).unwrap_or_default());}},"Export proof ↓"}
        }
        if s.proof_text{div{class:"proof-text","data-testid":"proof-text",pre{"{proof_text}"}}}
        else if !s.proof_raw{
            div{class:"collapsed-proof","data-testid":"proof-graph","aria-label":"Collapsed explanation of the actual proof",
                {render_collapsed(&run.collapsed_proof,0,&mut 150)}
            }
            p{class:"small muted",style:"margin-top:18px","This explanation folds the engine's role-level steps into surface statements. Open Raw graph for exact node identifiers, assertion links and execution cross-links. The full proof remains in the export."}
        }
        else if steps.is_empty(){div{class:"notice","This run returned no proof graph. Read the outcome and recorded execution for the available explanation."}}
        else{
            div{class:"proof-graph","aria-label":"Proof graph; each node links to its details","data-testid":"proof-graph",
                for row in rows.iter(){
                    {let id=row.id;let label=node_label(&steps[id]);let rule=steps[id].get("rule").unwrap_or(&Value::Null);let kind=text(rule,"type").replace('_'," ");let holds=steps[id].get("holds").and_then(Value::as_bool).unwrap_or(false);let sources=node_sources(&steps[id]).join(", #");
                    rsx!{div{class:"proof-node-wrap",style:"margin-left:{row.depth*18}px",
                        if row.reference{button{class:"text-action proof-reference",onclick:move |_|state.write().selected_node=Some(id),"↗ Shared node #{id} · {label}"}}
                        else{button{class:if id==node{"proof-node selected"}else{"proof-node"},"aria-pressed":id==node,onclick:move |_|state.write().selected_node=Some(id),
                            span{class:"kind","#{id} · {kind} · " if holds{"holds"}else{"not established"}}span{class:"name","{label}"}
                            if !sources.is_empty(){span{class:"links","Assertions #{sources}"}}
                        }}
                    }}}
                }
            }
            if rows.len()>=256{p{class:"notice","The graph display is limited to 256 rows. The text view and exported record retain the complete proof."}}
        }
        if s.proof_raw&&!s.proof_text{if let Some(step)=selected{
            section{class:"proof-details","aria-label":"Selected proof node details",span{class:"eyebrow","Proof node #{node}"}h3{"{node_label(step)}"}
                p{class:"small muted",style:"margin-top:10px","Connections are the child indices returned in this proof. Repeated nodes are shown as references to the same proof node."}
                div{class:"button-row",for id in node_sources(step){button{class:"button small",disabled:!can_link,onclick:move |_|{state.write().linked_assertion=Some(id.clone());open_pane(state,Pane::Knowledge);},"Assertion #{id} →"}}}
                if !can_link{p{class:"small muted",style:"margin-top:12px","This archived proof uses an earlier session's assertion identifiers. Its exported record retains those identities."}}
                {let linked:Vec<usize>=run.events().iter().enumerate().filter(|(_,e)|event_proof_node(e)==Some(node)).map(|(i,_)|i).collect();rsx!{
                    if linked.is_empty(){p{class:"small muted",style:"margin-top:14px","No recorded event links to this node in this recording."}}
                    else{div{class:"button-row",style:"margin-top:14px",for index in linked.into_iter().take(12){button{class:"text-action",onclick:move |_|{let mut s=state.write();s.show_change=false;s.selected_event=index;s.pane=Pane::Replay;s.viewed_replay=true;},"Execution event {index+1} →"}}}}
                }}
                details{style:"margin-top:16px",summary{"Complete node JSON"}pre{"{serde_json::to_string_pretty(step).unwrap_or_default()}"}}
            }
        }}
        details{style:"margin-top:24px",summary{"Accessible proof text"}pre{style:"margin-top:14px","{run.proof_text}"}}
    }
}

fn render_collapsed(node: &Value, depth: usize, budget: &mut usize) -> Element {
    if node.is_null() {
        return rsx! {p{class:"notice","No collapsed explanation was returned. Read the text view or inspect the raw graph."}};
    }
    if depth > 16 || *budget == 0 {
        return rsx! {p{class:"small muted","More detail is available in the raw graph and export."}};
    }
    *budget -= 1;
    let label = text(node, "label");
    let icon = text(node, "icon");
    let kind = text(node, "css_class");
    let holds = node.get("holds").and_then(Value::as_bool).unwrap_or(false);
    let children: Vec<Element> = node
        .get("children")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .map(|child| render_collapsed(child, depth + 1, budget))
                .collect()
        })
        .unwrap_or_default();
    if kind == "proof-role-detail" {
        return rsx! {details{class:"collapsed-detail",summary{"Role-level detail"}div{for child in children{ {child} }}}};
    }
    rsx! {div{class:"collapsed-branch",div{class:if holds{"collapsed-node holds"}else{"collapsed-node"},
        span{class:"collapsed-icon","aria-hidden":"true","{icon}"}div{span{class:"eyebrow",if holds{"Established in this derivation"}else{"Not established"}}p{"{label}"}}
    }if !children.is_empty(){div{class:"collapsed-children",for child in children{ {child} }}}}}
}

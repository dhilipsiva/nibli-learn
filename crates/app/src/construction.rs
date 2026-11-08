//! Views of captured compiler values. No parser or inference is recreated here.
use dioxus::prelude::*;
use serde_json::Value;

fn child_indices(node: &Value) -> Vec<usize> {
    let Some((kind, value)) = node.as_object().and_then(|o| o.iter().next()) else {
        return vec![];
    };
    let values: Vec<&Value> = match kind.as_str() {
        "AndNode" | "OrNode" => value
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        "ExistsNode" | "ForAllNode" => value.get(1).into_iter().collect(),
        "CountNode" => value.get(2).into_iter().collect(),
        "NotNode" | "PastNode" | "PresentNode" | "FutureNode" | "ObligatoryNode"
        | "PermittedNode" => vec![value],
        _ => vec![],
    };
    values
        .into_iter()
        .filter_map(|v| v.as_u64().and_then(|n| usize::try_from(n).ok()))
        .collect()
}

#[component]
pub fn CompilerArtifacts(record: Value) -> Element {
    let mut artifact_index = use_signal(|| 0_usize);
    let mut stage = use_signal(|| "source".to_string());
    let mut span_index = use_signal(|| 0_usize);
    let mut node_index = use_signal(|| 0_usize);
    let artifacts = record
        .get("artifacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let artifact = artifacts
        .get(artifact_index().min(artifacts.len().saturating_sub(1)))
        .cloned()
        .unwrap_or(Value::Null);
    let source = artifact["source"].as_str().unwrap_or("");
    let spans = artifact["statement_spans"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let span = spans.get(span_index());
    let range = span.and_then(|s| {
        Some((
            usize::try_from(s["start"].as_u64()?).ok()?,
            usize::try_from(s["end"].as_u64()?).ok()?,
        ))
    });
    let fragment = range.and_then(|(a, b)| source.get(a..b));
    let logic_key = if stage() == "lowered" {
        "lowered_logic"
    } else {
        "logic"
    };
    let nodes = artifact[logic_key]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let roots = artifact[logic_key]["roots"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let selected = node_index().min(nodes.len().saturating_sub(1));
    let node = nodes.get(selected).cloned().unwrap_or(Value::Null);
    let children = child_indices(&node);
    let parents: Vec<_> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| child_indices(n).contains(&selected))
        .map(|(i, _)| i)
        .collect();
    let stage_value = stage();
    rsx! {section{class:"compiler-inspector","aria-label":"Captured compiler artifacts",
        h3{"Follow the captured compiler values"}
        p{class:"small muted","These values came from the selected engine run. Changing this view does not compile or admit anything."}
        if artifacts.is_empty(){p{"No compiler artifact was captured in this record."}}
        else{
            div{class:"compiler-controls",
                label{r#for:"compiler-artifact","Artifact"}select{id:"compiler-artifact",value:"{artifact_index}",onchange:move|e|{if let Ok(i)=e.value().parse(){artifact_index.set(i);span_index.set(0);node_index.set(0);}},for(i,a)in artifacts.iter().enumerate(){option{value:"{i}",{a["id"].as_str().unwrap_or("unnamed")}}}}
                label{r#for:"compiler-stage","Compiler stage"}select{id:"compiler-stage",value:"{stage_value}",onchange:move|e|{stage.set(e.value());node_index.set(0);},option{value:"source","Source spans"}option{value:"parsed","Validated flat AST"}option{value:"lowered","Lowered logic"}option{value:"marked","Compute routing"}}
            }
            if stage_value=="source"{
                p{"{spans.len()} statement spans recorded as UTF-8 byte offsets."}
                div{class:"button-row",for(i,s)in spans.iter().take(128).enumerate(){button{class:"chip","aria-pressed":span_index()==i,onclick:move |_|span_index.set(i),{format!("Statement {} · {}–{}",i+1,s["start"],s["end"])}}}}
                if spans.len()>128{p{class:"small muted","The selector shows the first 128 spans. The complete artifact is in the record below."}}
                if let Some(fragment)=fragment{pre{"data-testid":"compiler-source-fragment","{fragment}"}}else{p{"No valid source span is selected. Inspect the complete source below."}}
                details{summary{"Complete source"}pre{"{source}"}}
            }else if stage_value=="parsed"{
                p{{format!("Root sentence indices: {}. Each array has its own index space; debug text describes the validated AST, not a token stream.",artifact["parsed_roots"])}}
                for(key,label)in[("parsed_predicates","Predicates"),("parsed_arguments","Arguments"),("parsed_sentences","Sentences")]{
                    details{open:true,summary{"{label}"}ol{start:"0",class:"compiler-array",if let Some(values)=artifact[key].as_array(){for value in values.iter().take(128){li{code{{value.as_str().unwrap_or("unavailable")}}}}if values.len()>128{li{"More values are available in the complete record."}}}}}
                }
            }else{
                if nodes.is_empty(){p{"This stage has no captured logic buffer."}}
                else{
                    p{"{nodes.len()} nodes. Follow roots and child indices through the actual flat buffer."}
                    div{class:"button-row",span{"Roots:"}for root in roots{if let Some(index)=root.as_u64().and_then(|n|usize::try_from(n).ok()).filter(|i|*i<nodes.len()){button{class:"chip",onclick:move |_|node_index.set(index),"Node {index}"}}}}
                    label{class:"node-selector",r#for:"compiler-node","Inspect node" input{id:"compiler-node",r#type:"number",min:"0",max:"{nodes.len()-1}",value:"{selected}",oninput:move |e|{if let Ok(i)=e.value().parse(){node_index.set(i);}}}}
                    pre{"data-testid":"compiler-node-value","{serde_json::to_string_pretty(&node).unwrap_or_default()}"}
                    div{class:"button-row",span{"Children:"}if children.is_empty(){span{"None"}}for index in children{if index<nodes.len(){button{class:"chip",onclick:move |_|node_index.set(index),"Child {index}"}}else{span{class:"badge warn","Out-of-range child {index}"}}}}
                    div{class:"button-row",span{"Parents:"}if parents.is_empty(){span{"None"}}for index in parents.iter().take(32).copied(){button{class:"chip",onclick:move |_|node_index.set(index),"Parent {index}"}}}
                    if parents.len()>1{p{class:"small muted","{parents.len()} parent nodes reference this same node index. That describes graph sharing, not allocation ownership."}}
                    p{class:"small muted","CountNode stores [variable, count, body index]; its count is not an edge. Predicate arguments are terms, not child nodes."}
                }
            }
        }
        details{summary{"Recorded compiler pipeline · source, AST debug arrays and lowered IR"}pre{{serde_json::to_string_pretty(&record["artifacts"]).unwrap_or_default()}}}
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn graph_edges_follow_variant_fields_and_do_not_turn_counts_or_terms_into_edges() {
        assert_eq!(child_indices(&json!({"CountNode":["$x",200,7]})), vec![7]);
        assert_eq!(child_indices(&json!({"ExistsNode":["$x",3]})), vec![3]);
        assert_eq!(child_indices(&json!({"AndNode":[2,2]})), vec![2, 2]);
        assert_eq!(child_indices(&json!({"NotNode":1})), vec![1]);
        assert!(child_indices(&json!({"Predicate":["sum",[{"number":7}]]})).is_empty());
    }
}

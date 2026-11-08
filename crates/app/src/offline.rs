use crate::state::{epoch, text};
use dioxus::prelude::*;
use nibli_learn_client::platform;
use nibli_learn_model::*;
use serde_json::Value;

#[component]
pub fn OfflinePage() -> Element {
    let mut catalog = use_signal(|| Value::Null);
    let mut status = use_signal(|| Value::Null);
    let mut progress = use_signal(|| Value::Null);
    let mut busy = use_signal(|| false);
    let mut job = use_signal(String::new);
    let mut message = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut verified = use_signal(|| None::<String>);
    use_future(move || async move {
        match platform::register_service_worker().await {
            Ok(_) => {}
            Err(e) => error.set(format!("{}: {}", e.kind, e.message)),
        }
        match platform::offline_catalog().await {
            Ok(v) => catalog.set(v),
            Err(e) => error.set(format!("{}: {}", e.kind, e.message)),
        }
        match platform::offline_status().await {
            Ok(v) => status.set(v),
            Err(e) => error.set(format!("{}: {}", e.kind, e.message)),
        }
    });
    let snapshot = status.read().clone();
    let entries = catalog.read()["catalog"]["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let installations = snapshot["installations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let current_revision = snapshot["active"]["revision"].as_str().map(str::to_owned);
    let installed = snapshot["installed"] == true;
    let runtime = snapshot["active"]["runtime_id"].as_str().unwrap_or("");
    let reload_needed = !runtime.is_empty() && runtime != RUNTIME_ID;
    rsx! {main{id:"main-content",tabindex:"-1",class:"packs-page",div{class:"page-heading",span{class:"eyebrow","Offline learning"}h1{"Take the investigation with you."}p{"Install the app runtime and original lesson content as a verified local pack. Lessons run in your browser without a model connection."}}div{class:"notice",strong{if installed{"Installed and verified for offline use."}else{"No complete active offline installation is confirmed."}}p{"A downloaded pack becomes active only after its runtime, content and version checks pass and you choose Activate."}}
        if !error.read().is_empty(){div{class:"notice error",role:"alert","{error}"}}if !message.read().is_empty(){div{class:"notice",role:"status","{message}"}}
        if catalog.read()["source"]=="retained"{p{class:"notice","Using the last retained catalog. Network updates are currently unavailable."}}
        for entry in entries{{let id=text(&entry,"id");let title=text(&entry,"title");rsx!{article{class:"data-card pack-card",span{class:"eyebrow","Runtime + original course"}h2{"{title}"}if entry["development"]==true{span{class:"badge warn","Development preview · not a release"}}p{"Foundations F1–F6, Use U1–U4, reference library and the actual reasoning worker."}button{class:"button primary",disabled:*busy.read(),onclick:move |_|{let entry=id.clone();busy.set(true);error.set(String::new());let job_id=format!("install-{}",epoch());job.set(job_id.clone());spawn(async move{let result=platform::install_offline(entry,job_id,move|v|progress.set(v)).await;match result{Ok(v)=>{let id=text(&v,"id");match validate_content(id.clone()).await{Ok(())=>{verified.set(Some(id));message.set("Download verified. Review the version below, then activate when ready.".into());},Err(e)=>error.set(e)}},Err(e)=>error.set(format!("{}: {} The previous active pack was retained.",e.kind,e.message))}busy.set(false);if let Ok(v)=platform::offline_status().await{status.set(v);}});},"Download and verify"}}}}}
        if *busy.read(){section{class:"data-card",h2{"Installing verified assets"}p{{format!("Phase: {}",text(&progress.read(),"phase"))}}p{{format!("Files {} / {} · {} bytes received",text(&progress.read(),"completed"),text(&progress.read(),"total"),text(&progress.read(),"bytes"))}}progress{max:progress.read()["total"].as_u64().unwrap_or(1) as f64,value:progress.read()["completed"].as_u64().unwrap_or(0) as f64}button{class:"button",onclick:move |_|{platform::cancel_installation(&job.read());message.set("Cancellation requested; the existing active pack remains available.".into());},"Cancel installation"}}}
        section{class:"data-card",h2{"Verified local installations"}if installations.is_empty(){p{"No verified installations are stored yet."}}for install in installations{{let id=text(&install,"id");let runtime_id=text(&install,"runtime_id");let active=install["active"]==true;let complete=install["complete"]==true;let expected=current_revision.clone();let id_for_click=id.clone();rsx!{article{class:"installation-row",h3{{format!("Content version {}",text(&install,"content_version"))}}if install["development"]==true{span{class:"badge warn","Development preview · not a release"}}p{class:"small muted","Runtime {runtime_id}"}span{class:if active&&complete{"badge ok"}else{"badge warn"},if active&&complete{"Active · verified"}else if complete{"Verified · not active"}else{"Incomplete · reinstall required"}}details{summary{"Version and integrity metadata"}pre{"{serde_json::to_string_pretty(&install).unwrap_or_default()}"}}button{class:"button",disabled:*busy.read()||active||!complete,onclick:move |_|{let id=id_for_click.clone();let expected=expected.clone();busy.set(true);spawn(async move{let result=validate_content(id.clone()).await;match result{Ok(())=>match platform::activate_offline(id,expected).await{Ok(_)=>{message.set("Verified installation activated. Reload to use its runtime and content together.".into());if let Ok(v)=platform::offline_status().await{status.set(v);}},Err(e)=>error.set(format!("{}: {}",e.kind,e.message))},Err(e)=>error.set(e)}busy.set(false);});},"Activate this verified version"}}}}}
        if snapshot["active"]["previous_id"].is_string(){button{class:"button",disabled:*busy.read(),onclick:move |_|{let Some(expected)=status.read()["active"]["revision"].as_str().map(str::to_owned)else{return;};busy.set(true);spawn(async move{match platform::undo_offline(expected).await{Ok(_)=>{message.set("Previous verified installation restored. Reload to use it.".into());if let Ok(v)=platform::offline_status().await{status.set(v);}},Err(e)=>error.set(format!("{}: {}",e.kind,e.message))}busy.set(false);});},"Roll back to previous verified version"}}
        }
        if reload_needed{div{class:"notice",p{"This page remains bound to its original runtime. Reload to use the newly active installation."}button{class:"button primary",onclick:move |_|{spawn(async move{let _=document::eval("window.location.reload(); return true;").await;});},"Reload active installation"}}}
        if verified.read().is_some(){p{class:"small muted","Verified download is ready. Activation is a separate choice above."}}
        section{class:"data-card",h2{"What offline means here"}p{"The engine, original lessons and installed references run locally. Public source links still need a network connection. Browser storage can be cleared or evicted; keep exported copies of important progress and projects."}p{"A failed download, integrity error, incompatible update or cancelled install does not replace the current active pack."}}
    }}
}
async fn validate_content(id: String) -> Result<(), String> {
    let value = platform::installed_content(id)
        .await
        .map_err(|e| format!("{}: {}", e.kind, e.message))?;
    let course = Course::parse(&value["course"].to_string())?;
    let references = ReferenceLibrary::parse(&value["references"].to_string(), &course)?;
    if course.schema >= 3 {
        LabLibrary::parse(&value["labs"].to_string())?.validate_links(&course, &references)?;
    }
    Ok(())
}

mod assurance;
mod compiler;
mod compute;
mod measurement;
mod ontology;
mod rules;
mod runtime;
mod scoped;
mod store;
use serde_json::json;
fn main() {
    let name = std::env::args()
        .nth(1)
        .expect("choose a local lab name from labs/run.py");
    let observations = match name.as_str() {
        "compute" => compute::run(),
        "measurement" => measurement::run(),
        "ontology" => ontology::run(),
        "scoped-application" => scoped::run(),
        "compiler-internals" => compiler::run(),
        "store-search" => store::run(),
        "rules-witnesses" => rules::run(),
        "runtime-buffers" => runtime::run(),
        "finite-assurance" => assurance::run(),
        _ => panic!("unknown local lab: {name}"),
    };
    println!("{}", serde_json::to_string_pretty(&json!({"schema":1,"lab_id":name,"engine_revision":"0440db4233f8a8cffbf81be9654a5e54c1f9c836","observations":observations})).unwrap());
}

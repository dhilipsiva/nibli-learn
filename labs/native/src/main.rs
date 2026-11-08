mod compute;
mod measurement;
mod ontology;
mod scoped;
use serde_json::json;
fn main() {
    let name = std::env::args()
        .nth(1)
        .expect("choose compute, measurement, ontology or scoped-application");
    let observations = match name.as_str() {
        "compute" => compute::run(),
        "measurement" => measurement::run(),
        "ontology" => ontology::run(),
        "scoped-application" => scoped::run(),
        _ => panic!("unknown local lab: {name}"),
    };
    println!("{}", serde_json::to_string_pretty(&json!({"schema":1,"lab_id":name,"engine_revision":"0440db4233f8a8cffbf81be9654a5e54c1f9c836","observations":observations})).unwrap());
}

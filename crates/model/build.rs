use std::{env, fs, path::PathBuf};

fn main() {
    for name in ["NIBLI_APP_REVISION", "NIBLI_RUNTIME_ID"] {
        println!("cargo:rerun-if-env-changed={name}");
        println!(
            "cargo:rustc-env={name}={}",
            env::var(name).unwrap_or_default()
        );
    }
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", path.display());
    let lock = fs::read_to_string(path).expect("The workspace lockfile identifies the engine");
    let package = lock
        .split("[[package]]")
        .find(|entry| entry.lines().any(|line| line == "name = \"nibli-session\""))
        .expect("The lockfile must contain nibli-session");
    let field = |name: &str| {
        package.lines().find_map(|line| {
            line.strip_prefix(&format!("{name} = \""))
                .and_then(|value| value.strip_suffix('"'))
        })
    };
    println!("cargo:rerun-if-env-changed=NIBLI_BUILD_ENGINE_REVISION");
    let development_revision =
        env::var("NIBLI_BUILD_ENGINE_REVISION").unwrap_or_else(|_| "local-development".into());
    let revision = field("source")
        .and_then(|source| source.rsplit_once('#').map(|(_, revision)| revision))
        .unwrap_or(&development_revision);
    println!("cargo:rustc-env=NIBLI_ENGINE_REVISION={revision}");
    println!(
        "cargo:rustc-env=NIBLI_ENGINE_VERSION={}",
        field("version").unwrap()
    );
}

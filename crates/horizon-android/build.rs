//! Embed only Horizon's original preparation tools, never generated game data.
use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let directory = root.join("mods/horizon/src/test/java/dev/horizonlab/parity");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut files: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "java"))
        .collect();
    files.push(root.join("research-lock.json"));
    files.sort();
    let mut source = String::from("const PREPARE_SOURCES: &[(&str, &[u8])] = &[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
        source.push_str(&format!(
            "({relative:?}, include_bytes!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    source.push_str("];\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("prepare_sources.rs"),
        source,
    )
    .unwrap();
}

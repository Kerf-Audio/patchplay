//! Embeds the factory presets: every `presets/*.kerf` in the workspace becomes an `include_str!`
//! entry, sorted by name, so the desktop app and the browser build ship the same list with no
//! folder lookup at run time.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let dir = manifest.join("../../presets");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut files: Vec<(String, PathBuf)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "kerf"))
        .filter_map(|p| Some((p.file_stem()?.to_str()?.to_owned(), p)))
        .collect();
    files.sort();

    let mut out = String::from("pub(crate) const PRESETS: &[(&str, &str)] = &[\n");
    for (name, path) in files {
        let path = fs::canonicalize(&path).expect("preset exists");
        out.push_str(&format!(
            "    ({name:?}, include_str!({:?})),\n",
            path.display().to_string()
        ));
    }
    out.push_str("];\n");
    let target = PathBuf::from(env::var_os("OUT_DIR").expect("set by cargo")).join("presets.rs");
    fs::write(target, out).expect("writing the preset list");
}

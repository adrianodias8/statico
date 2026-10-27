//! Embeds the starter templates in `templates/` into the binary, so
//! `statico new my-site --template blog` works offline and always matches the
//! installed version.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn files(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name == ".DS_Store" || name == "_site" || name == "node_modules" {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            files(&path, base, out);
        } else {
            let rel = path.strip_prefix(base).unwrap();
            let rel = rel.to_string_lossy().replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("templates");
    println!("cargo:rerun-if-changed=templates");
    let mut code = String::from(
        "/// A template's files: path inside the template, then its bytes.\n\
         pub type Files = &'static [(&'static str, &'static [u8])];\n\
         pub static EMBEDDED: &[(&str, Files)] = &[\n",
    );
    let mut names: Vec<_> = std::fs::read_dir(&root)
        .map(|r| r.flatten().filter(|e| e.path().is_dir()).collect())
        .unwrap_or_default();
    names.sort_by_key(|e| e.file_name());
    for t in names {
        let mut list = Vec::new();
        files(&t.path(), &t.path(), &mut list);
        writeln!(code, "    ({:?}, &[", t.file_name().to_string_lossy()).unwrap();
        for (rel, path) in list {
            writeln!(
                code,
                "        ({rel:?}, include_bytes!({:?})),",
                path.display().to_string()
            )
            .unwrap();
        }
        code.push_str("    ]),\n");
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("templates.rs");
    std::fs::write(out, code).unwrap();
}

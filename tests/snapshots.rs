//! A snapshot of every file the kitchen-sink fixture builds. Any change in
//! output shows up as a diff to review (`cargo insta review`), which is the
//! promise `statico diff` makes to users: an upgrade never changes a site
//! silently.

use statico::build::{BuildOptions, build};
use std::path::Path;

#[test]
fn kitchen_sink_output() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kitchen-sink");
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("site");
    build(
        &fixture,
        BuildOptions {
            url: Some("https://example.com/".into()),
            out: Some(out.clone()),
            skip_tools: true,
            minify: Some(false),
            hash_assets: None,
        },
    )
    .unwrap();

    let mut files: Vec<String> = walkdir::WalkDir::new(&out)
        .into_iter()
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            e.path()
                .strip_prefix(&out)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    files.sort();

    // The list of files first, so an added or missing page is the first diff.
    let mut manifest = String::new();
    for rel in &files {
        let bytes = std::fs::read(out.join(rel)).unwrap();
        match std::str::from_utf8(&bytes) {
            Ok(_) => manifest.push_str(&format!("{rel}\n")),
            Err(_) => {
                let sha = ring::digest::digest(&ring::digest::SHA256, &bytes);
                let hex: String = sha.as_ref().iter().map(|b| format!("{b:02x}")).collect();
                manifest.push_str(&format!("{rel}  {} bytes  sha256 {hex}\n", bytes.len()));
            }
        }
    }
    insta::assert_snapshot!("files", manifest);

    for rel in &files {
        if let Ok(text) = std::fs::read_to_string(out.join(rel)) {
            insta::assert_snapshot!(rel.replace('/', "__"), text);
        }
    }
}

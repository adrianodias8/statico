//! `statico diff <old> <new>`: compare two build outputs. Build with the old
//! Statico version into one folder and the new one into another; an empty diff
//! means the upgrade changed nothing.

use anyhow::{Result, bail};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize, Default)]
pub struct DiffReport {
    pub ok: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub unchanged: usize,
}

/// Fingerprinted names (`main.1a2b3c4d.css`) are normalised so a content
/// change shows up as "changed", not "removed + added".
fn normalize(rel: &str) -> String {
    let parts: Vec<&str> = rel.rsplitn(3, '.').collect();
    if parts.len() == 3 && parts[1].len() == 8 && parts[1].chars().all(|c| c.is_ascii_hexdigit()) {
        format!("{}.{}", parts[2], parts[0])
    } else {
        rel.to_string()
    }
}

fn index(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    if !dir.is_dir() {
        bail!("{} is not a directory", dir.display());
    }
    let mut m = BTreeMap::new();
    for e in walkdir::WalkDir::new(dir) {
        let e = e?;
        if e.file_type().is_file() {
            let rel = e
                .path()
                .strip_prefix(dir)?
                .to_string_lossy()
                .replace('\\', "/");
            m.insert(normalize(&rel), std::fs::read(e.path())?);
        }
    }
    Ok(m)
}

pub fn diff(old: &Path, new: &Path) -> Result<DiffReport> {
    let a = index(old)?;
    let b = index(new)?;
    let mut r = DiffReport::default();
    for (k, v) in &a {
        match b.get(k) {
            None => r.removed.push(k.clone()),
            Some(w) if w != v => r.changed.push(k.clone()),
            Some(_) => r.unchanged += 1,
        }
    }
    for k in b.keys() {
        if !a.contains_key(k) {
            r.added.push(k.clone());
        }
    }
    r.ok = r.added.is_empty() && r.removed.is_empty() && r.changed.is_empty();
    Ok(r)
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalizes_fingerprints() {
        assert_eq!(
            super::normalize("assets/css/main.1a2b3c4d.css"),
            "assets/css/main.css"
        );
        assert_eq!(super::normalize("blog/index.html"), "blog/index.html");
    }
}

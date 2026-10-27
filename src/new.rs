//! `statico new <folder> [--template <name|owner/repo|url>]`

use anyhow::{Context, Result, bail};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/templates.rs"));
}

/// A template that ships inside the binary (from `templates/` in the repo).
pub struct Builtin {
    pub slug: &'static str,
    pub about: &'static str,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin {
        slug: "portfolio",
        about: "Freelancer portfolio: services, experience, projects, writing",
    },
    Builtin {
        slug: "blog",
        about: "Blog: posts, tags, pagination, feed",
    },
    Builtin {
        slug: "docs",
        about: "Documentation: sidebar, on-this-page, search, prev/next",
    },
];

/// The files of a built-in template, if `spec` names one.
pub fn builtin(spec: &str) -> Option<&'static [(&'static str, &'static [u8])]> {
    let spec = spec
        .trim()
        .trim_start_matches("--template=")
        .trim_start_matches("--");
    embedded::EMBEDDED
        .iter()
        .find(|(name, _)| *name == spec)
        .map(|(_, files)| *files)
}

/// Templates downloaded from GitHub (DevDojo's, made for Static).
pub struct Template {
    pub slug: &'static str,
    pub repo: &'static str,
    pub about: &'static str,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        slug: "starter",
        repo: "static-templates/starter",
        about: "Minimal starting point (default)",
    },
    Template {
        slug: "aria",
        repo: "static-templates/aria",
        about: "Landing page",
    },
    Template {
        slug: "boxed",
        repo: "static-templates/boxed",
        about: "Boxed landing page",
    },
    Template {
        slug: "stone",
        repo: "static-templates/stone",
        about: "Portfolio / agency",
    },
];

/// Resolve a template argument to a zip URL of the repo's default branch.
pub fn template_zip_url(spec: &str) -> Result<String> {
    let spec = spec
        .trim()
        .trim_start_matches("--template=")
        .trim_start_matches("--");
    let repo = if let Some(t) = TEMPLATES.iter().find(|t| t.slug == spec) {
        t.repo.to_string()
    } else if let Some(rest) = spec
        .strip_prefix("https://github.com/")
        .or_else(|| spec.strip_prefix("github.com/"))
    {
        rest.trim_end_matches('/')
            .trim_end_matches(".git")
            .to_string()
    } else if spec.starts_with("https://") && spec.ends_with(".zip") {
        return Ok(spec.to_string());
    } else if spec.split('/').count() == 2 && !spec.contains(' ') {
        spec.to_string()
    } else {
        let names: Vec<_> = TEMPLATES.iter().map(|t| t.slug).collect();
        bail!(
            "unknown template `{spec}`. Use one of: {}, or `owner/repo` for any GitHub repository.",
            names.join(", ")
        );
    };
    // `HEAD` resolves to the default branch, whether it's `main` or `master`.
    Ok(format!("https://github.com/{repo}/archive/HEAD.zip"))
}

fn download(url: &str) -> Result<Vec<u8>> {
    use ureq::tls::{RootCerts, TlsConfig};
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    let mut resp = agent
        .get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let mut buf = Vec::new();
    resp.body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .reader()
        .read_to_end(&mut buf)
        .context("reading template archive")?;
    Ok(buf)
}

/// Extract, stripping the top-level `<repo>-<branch>/` folder. Rejects
/// entries that would escape the destination ("zip slip").
pub fn extract(zip_bytes: &[u8], dest: &Path) -> Result<usize> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(zip_bytes)).context("template is not a valid zip file")?;
    let mut n = 0;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let Some(name) = f.enclosed_name() else {
            continue;
        };
        let rel: PathBuf = name.components().skip(1).collect();
        if rel.as_os_str().is_empty()
            || rel.components().any(|c| !matches!(c, Component::Normal(_)))
        {
            continue;
        }
        let target = dest.join(&rel);
        if f.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut out = std::fs::File::create(&target)
            .with_context(|| format!("writing {}", target.display()))?;
        std::io::copy(&mut f, &mut out)?;
        #[cfg(unix)]
        if let Some(mode) = f.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode & 0o755));
        }
        n += 1;
    }
    Ok(n)
}

fn write_builtin(files: &[(&str, &[u8])], dest: &Path) -> Result<usize> {
    for (rel, bytes) in files {
        let target = dest.join(rel);
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&target, bytes).with_context(|| format!("writing {}", target.display()))?;
    }
    Ok(files.len())
}

/// Copy a local template folder, skipping VCS, dependencies and build output.
fn copy_template_dir(src: &Path, dest: &Path) -> Result<usize> {
    let mut n = 0;
    let skip = [".git", "node_modules", "_site"];
    let walker = walkdir::WalkDir::new(src)
        .into_iter()
        .filter_entry(|e| !skip.iter().any(|s| e.file_name() == *s));
    for e in walker {
        let e = e?;
        let rel = e.path().strip_prefix(src)?;
        let target = dest.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if e.file_type().is_file() {
            std::fs::copy(e.path(), &target)?;
            n += 1;
        }
    }
    Ok(n)
}

pub fn new_project(folder: &str, template: &str, install: bool) -> Result<PathBuf> {
    let dest = PathBuf::from(folder);
    if dest.exists() && std::fs::read_dir(&dest)?.next().is_some() {
        bail!("`{folder}` already exists and is not empty");
    }
    let local = Path::new(template.trim_start_matches("--template="));
    // A bare name is a built-in template even if a folder of that name happens
    // to exist here; a local folder is written as a path (`./blog`).
    let bare = !template.contains(['/', '\\']) && !template.ends_with(".zip");
    let n = if let (true, Some(files)) = (bare, builtin(template)) {
        std::fs::create_dir_all(&dest)?;
        write_builtin(files, &dest)?
    } else if local.is_dir() {
        // A local folder (e.g. your team's own starter).
        println!("Copying template from {} …", local.display());
        std::fs::create_dir_all(&dest)?;
        copy_template_dir(local, &dest)?
    } else if local.is_file() && local.extension().is_some_and(|e| e == "zip") {
        std::fs::create_dir_all(&dest)?;
        extract(&std::fs::read(local)?, &dest)?
    } else {
        let url = template_zip_url(template)?;
        println!("Downloading template from {url} …");
        let bytes = download(&url)?;
        std::fs::create_dir_all(&dest)?;
        extract(&bytes, &dest)?
    };
    if !dest.join("pages").is_dir() {
        eprintln!(
            "warning: the template has no `pages` folder; is it a Statico (or Static) template?"
        );
    }
    println!("Created {folder} ({n} files)");

    if install
        && dest.join("package.json").is_file()
        && let Some(npm) = crate::assets::which("npm")
    {
        println!("Installing npm dependencies …");
        let ok = std::process::Command::new(npm)
            .args(["install", "--no-fund", "--no-audit", "--loglevel=error"])
            .current_dir(&dest)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("warning: `npm install` failed; run it yourself inside {folder}");
        }
    }
    println!("\nNext steps:\n  cd {folder}\n  statico dev\n");
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn urls() {
        assert_eq!(
            template_zip_url("aria").unwrap(),
            "https://github.com/static-templates/aria/archive/HEAD.zip"
        );
        assert_eq!(
            template_zip_url("--aria").unwrap(),
            "https://github.com/static-templates/aria/archive/HEAD.zip"
        );
        assert_eq!(
            template_zip_url("--template=me/site").unwrap(),
            "https://github.com/me/site/archive/HEAD.zip"
        );
        assert_eq!(
            template_zip_url("https://github.com/me/site.git").unwrap(),
            "https://github.com/me/site/archive/HEAD.zip"
        );
        assert!(template_zip_url("nope").is_err());
    }

    #[test]
    fn every_builtin_is_embedded_and_described() {
        let embedded: Vec<_> = embedded::EMBEDDED.iter().map(|(n, _)| *n).collect();
        let described: Vec<_> = BUILTINS.iter().map(|b| b.slug).collect();
        let mut a = embedded.clone();
        let mut b = described.clone();
        a.sort();
        b.sort();
        assert_eq!(a, b, "templates/ and BUILTINS disagree");
        for (name, files) in embedded::EMBEDDED {
            assert!(
                files.iter().any(|(p, _)| *p == "pages/index.html"),
                "{name} has no pages/index.html"
            );
        }
    }

    #[test]
    fn extract_strips_root_and_blocks_slip() {
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(Cursor::new(&mut buf));
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("repo-main/pages/index.html", o).unwrap();
            z.write_all(b"hi").unwrap();
            z.start_file("repo-main/../../evil.txt", o).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        let d = tempfile::tempdir().unwrap();
        let n = extract(&buf, d.path()).unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            std::fs::read_to_string(d.path().join("pages/index.html")).unwrap(),
            "hi"
        );
    }
}

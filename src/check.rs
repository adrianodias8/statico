//! `statico check`: build into a temporary folder, then verify the output.
//! The agent's (and CI's) definition of done: zero errors.

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::build::{BuildError, BuildOptions, build};
use crate::diag::Diagnostic;
use crate::template::tags::find_tags;

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub ok: bool,
    pub pages: usize,
    pub links_checked: usize,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

fn issue(
    level: &'static str,
    code: &'static str,
    page: &crate::build::PageInfo,
    message: String,
) -> Diagnostic {
    Diagnostic {
        level,
        file: Some(page.source.clone()),
        line: None,
        url: Some(page.url.clone()),
        message,
        code: Some(code),
    }
}

/// Resolve a URL path to a file in the build output, if it exists.
fn target_exists(out: &Path, path: &str) -> Option<PathBuf> {
    let p = crate::site::percent_decode(path);
    let rel = p.trim_start_matches('/');
    let base = out.join(rel);
    let candidates = if rel.is_empty() || p.ends_with('/') {
        vec![base.join("index.html")]
    } else {
        vec![
            base.clone(),
            base.join("index.html"),
            out.join(format!("{rel}.html")),
        ]
    };
    candidates.into_iter().find(|c| c.is_file())
}

static IDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\s(?:id|name)\s*=\s*["']([^"']+)["']"#).unwrap());
static UNRESOLVED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{(?:frontmatter|site|page|env|collections|term|pagination|content\.[\w-]+|\$loop)\.[\w.$-]+(?:\|[^}]*)?\}").unwrap()
});

pub fn check(root: &Path, url: Option<String>, strict: bool) -> Result<CheckReport> {
    let tmp = tempdir()?;
    let out = tmp.join("site");
    let was_quiet = crate::diag::quiet();
    crate::diag::set_quiet(true);
    let built = build(
        root,
        BuildOptions {
            url,
            out: Some(out.clone()),
            skip_tools: true,
            minify: Some(false),
            hash_assets: Some(false),
        },
    );
    crate::diag::set_quiet(was_quiet);
    let report = match built {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            if let Some(be) = e.downcast_ref::<BuildError>() {
                return Ok(CheckReport {
                    ok: false,
                    pages: 0,
                    links_checked: 0,
                    errors: be.errors.clone(),
                    warnings: crate::diag::take_warnings(),
                });
            }
            return Err(e);
        }
    };
    let base = report.out_dir.clone();
    let site_base = crate::site::Site::load(root, crate::site::Mode::Build, None)
        .map(|s| s.base_url)
        .unwrap_or_else(|_| "relative".into());

    let mut errors = Vec::new();
    // `check` builds with relative URLs on purpose; that feed warning is noise here.
    let mut warnings: Vec<Diagnostic> = report
        .warnings
        .iter()
        .filter(|w| !w.message.contains("feed links are relative"))
        .cloned()
        .collect();
    let mut id_cache: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut ids_of = |file: &Path| -> HashSet<String> {
        id_cache
            .entry(file.to_path_buf())
            .or_insert_with(|| {
                let html = std::fs::read_to_string(file).unwrap_or_default();
                IDS.captures_iter(&html).map(|c| c[1].to_string()).collect()
            })
            .clone()
    };
    let mut links_checked = 0;
    let mut titles: HashMap<String, Vec<String>> = HashMap::new();

    for page in &report.page_info {
        let file = base.join(&page.file);
        let html = std::fs::read_to_string(&file).unwrap_or_default();
        let label = page;

        // <title>
        let title = html.find("<title>").and_then(|s| {
            html[s + 7..]
                .find("</title>")
                .map(|e| html[s + 7..s + 7 + e].trim().to_string())
        });
        match title.as_deref() {
            None | Some("") => warnings.push(issue(
                "warning",
                "S103",
                label,
                "page has no <title>".into(),
            )),
            Some(t) => titles
                .entry(t.to_string())
                .or_default()
                .push(page.url.clone()),
        }
        if strict && !html.contains("name=\"description\"") {
            warnings.push(issue(
                "warning",
                "S104",
                label,
                "page has no meta description".into(),
            ));
        }
        if let Some(h) = find_tags(&html, &["html"]).into_iter().find(|t| !t.closing)
            && h.attr("lang").is_none()
        {
            warnings.push(issue(
                "warning",
                "S107",
                label,
                "<html> has no lang attribute".into(),
            ));
        }
        // Unresolved placeholders outside <script>.
        let visible = strip_scripts(&html);
        for m in UNRESOLVED.find_iter(&visible).take(5) {
            warnings.push(issue(
                "warning",
                "S106",
                label,
                format!("unresolved placeholder `{}`", m.as_str()),
            ));
        }

        for t in find_tags(&html, &["a", "link", "img", "script", "source", "iframe"]) {
            if t.closing {
                continue;
            }
            if t.name == "img" && t.attr("alt").is_none() {
                warnings.push(issue(
                    "warning",
                    "S102",
                    label,
                    format!(
                        "<img src=\"{}\"> has no alt text",
                        t.attr("src").unwrap_or("")
                    ),
                ));
            }
            let attr = if matches!(t.name.as_str(), "a" | "link") {
                "href"
            } else {
                "src"
            };
            let Some(href) = t.attr(attr) else { continue };
            let href = href.trim();
            if href.is_empty()
                || href.starts_with("mailto:")
                || href.starts_with("tel:")
                || href.starts_with("javascript:")
                || href.starts_with("data:")
                || href.starts_with("//")
            {
                continue;
            }
            let href = if site_base != "relative" && href.starts_with(&site_base) {
                &href[site_base.len()..]
            } else {
                href
            };
            if href.starts_with("http://") || href.starts_with("https://") || href.contains('{') {
                continue;
            }
            links_checked += 1;
            let (path, frag) = match href.split_once('#') {
                Some((p, f)) => (p, Some(f)),
                None => (href, None),
            };
            let path = path.split('?').next().unwrap_or("");
            let resolved = if path.is_empty() {
                Some(file.clone())
            } else {
                let abs = if path.starts_with('/') {
                    path.to_string()
                } else {
                    // relative to the page's URL directory
                    let dir = if page.url.ends_with('/') {
                        page.url.clone()
                    } else {
                        page.url
                            .rsplit_once('/')
                            .map(|(d, _)| format!("{d}/"))
                            .unwrap_or("/".into())
                    };
                    normalize(&format!("{dir}{path}"))
                };
                target_exists(&base, &abs)
            };
            match resolved {
                None => errors.push(issue(
                    "error",
                    "S101",
                    label,
                    format!("broken link: {href}"),
                )),
                Some(target) => {
                    if let Some(f) = frag.filter(|f| !f.is_empty())
                        && target.extension().is_some_and(|e| e == "html")
                        && !ids_of(&target).contains(&crate::site::percent_decode(f))
                    {
                        errors.push(issue(
                            "error",
                            "S105",
                            label,
                            format!("missing anchor: {href}"),
                        ));
                    }
                }
            }
        }
    }
    for (title, urls) in titles {
        if urls.len() > 1 && strict {
            warnings.push(Diagnostic {
                level: "warning",
                file: None,
                line: None,
                url: None,
                message: format!(
                    "{} pages share the title \"{title}\": {}",
                    urls.len(),
                    urls.join(", ")
                ),
                code: Some("S103"),
            });
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    let ok = errors.is_empty() && (!strict || warnings.is_empty());
    Ok(CheckReport {
        ok,
        pages: report.pages,
        links_checked,
        errors,
        warnings,
    })
}

fn strip_scripts(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find("<script") {
        out.push_str(&rest[..i]);
        match rest[i..].find("</script>") {
            Some(e) => rest = &rest[i + e + 9..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let trailing = if path.ends_with('/') && !parts.is_empty() {
        "/"
    } else {
        ""
    };
    format!("/{}{trailing}", parts.join("/"))
}

fn tempdir() -> Result<PathBuf> {
    let p = std::env::temp_dir().join(format!(
        "statico-check-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalizes() {
        assert_eq!(super::normalize("/blog/../about/"), "/about/");
        assert_eq!(super::normalize("/a/./b"), "/a/b");
    }
}

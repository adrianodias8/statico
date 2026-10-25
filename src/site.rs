//! A Static project on disk: routing, data loading and page rendering.
//! Shared by `static dev` (render on request) and `static build`.

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, RwLock};
use walkdir::WalkDir;

use crate::config::{Config, load_env};
use crate::markdown;
use crate::template::{self, DataSource, Scope, compose::Composer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dev,
    Build,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tailwind {
    /// `@tailwind base;` + `tailwind.config.js` (all official templates today).
    V3,
    /// `@import "tailwindcss";`
    V4,
    /// No `assets/css/main.css`.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Page(PathBuf),
    Content(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Route {
    /// URL path, e.g. `/blog/hello/`.
    pub url: String,
    /// Output file relative to the build dir, e.g. `blog/hello/index.html`.
    pub out: PathBuf,
    pub source: Source,
}

pub struct Site {
    pub root: PathBuf,
    pub config: Config,
    pub mode: Mode,
    /// `relative` or an absolute URL without trailing slash.
    pub base_url: String,
    pub tailwind: Tailwind,
    /// Set by the build once main.css has been compiled.
    pub css_compiled: std::sync::atomic::AtomicBool,
    env: Map<String, Value>,
    cache: RwLock<HashMap<String, Value>>,
}

impl Site {
    pub fn load(root: &Path, mode: Mode, url_override: Option<String>) -> Result<Self> {
        let root = root
            .canonicalize()
            .with_context(|| format!("cannot open project at {}", root.display()))?;
        if !root.join("pages").is_dir() {
            bail!(
                "no `pages` directory in {}\n  Run this inside a Static project, or create one with `static new my-site`.",
                root.display()
            );
        }
        let config = Config::load(&root)?;
        let cfg_url = match mode {
            Mode::Build => config.build.url.clone(),
            Mode::Dev => config.dev.url.clone(),
        };
        let base_url = url_override
            .or(cfg_url)
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "relative".into())
            .trim_end_matches('/')
            .to_string();
        let base_url = if base_url.is_empty() {
            "relative".into()
        } else {
            base_url
        };
        let tailwind = detect_tailwind(&root);
        let env = load_env(&root);
        Ok(Site {
            root,
            config,
            mode,
            base_url,
            tailwind,
            css_compiled: false.into(),
            env,
            cache: RwLock::new(HashMap::new()),
        })
    }

    pub fn clear_cache(&self) {
        self.cache.write().unwrap().clear();
    }

    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .unwrap_or(p)
            .display()
            .to_string()
            .replace('\\', "/")
    }

    fn cached(&self, key: &str, f: impl FnOnce() -> Result<Value>) -> Result<Value> {
        if let Some(v) = self.cache.read().unwrap().get(key) {
            return Ok(v.clone());
        }
        let v = f()?;
        self.cache
            .write()
            .unwrap()
            .insert(key.to_string(), v.clone());
        Ok(v)
    }

    // ---------------------------------------------------------------- routing

    /// Every page the build should write.
    pub fn routes(&self) -> Result<Vec<Route>> {
        let mut routes: Vec<Route> = Vec::new();
        let pages = self.root.join("pages");
        for e in WalkDir::new(&pages).sort_by_file_name() {
            let e = e?;
            let p = e.path();
            if !e.file_type().is_file() || p.extension().and_then(|x| x.to_str()) != Some("html") {
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy();
            if name.contains('[') {
                continue; // `[content].html` and friends are templates, not routes
            }
            let rel = p.strip_prefix(&pages)?.with_extension("");
            let rel = to_url_path(&rel);
            let (url, out) = if rel == "404" {
                ("/404.html".to_string(), PathBuf::from("404.html"))
            } else {
                route_for(&rel)
            };
            routes.push(Route {
                url,
                out,
                source: Source::Page(p.to_path_buf()),
            });
        }

        let content = self.root.join("content");
        if content.is_dir() {
            for e in WalkDir::new(&content).sort_by_file_name() {
                let e = e?;
                let p = e.path();
                if !e.file_type().is_file() || p.extension().and_then(|x| x.to_str()) != Some("md")
                {
                    continue;
                }
                if self.mode == Mode::Build && self.is_draft(p)? {
                    continue;
                }
                let rel = to_url_path(&p.strip_prefix(&content)?.with_extension(""));
                let (url, out) = route_for(&rel);
                // Content wins over a page with the same URL (as in `static dev`).
                if let Some(existing) = routes.iter().position(|r| r.url == url) {
                    eprintln!(
                        "warning: {} and {} both map to {url}; using the markdown file",
                        self.rel(match &routes[existing].source {
                            Source::Page(p) | Source::Content(p) => p,
                        }),
                        self.rel(p)
                    );
                    routes.remove(existing);
                }
                routes.push(Route {
                    url,
                    out,
                    source: Source::Content(p.to_path_buf()),
                });
            }
        }
        Ok(routes)
    }

    fn is_draft(&self, md: &Path) -> Result<bool> {
        let src = std::fs::read_to_string(md)?;
        let doc = markdown::parse_front_matter(&src, &self.rel(md))?;
        Ok(doc
            .front_matter
            .get("draft")
            .is_some_and(template::expr::truthy))
    }

    /// Map a request path to a source file (used by the dev server).
    pub fn resolve(&self, url_path: &str) -> Option<Source> {
        let clean = sanitize_url_path(url_path)?;
        let route = if clean.is_empty() {
            "index".to_string()
        } else {
            clean
        };
        let content = self.root.join("content");
        let pages = self.root.join("pages");
        let candidates = [
            Source::Content(content.join(format!("{route}.md"))),
            Source::Content(content.join(&route).join("index.md")),
            Source::Page(pages.join(format!("{route}.html"))),
            Source::Page(pages.join(&route).join("index.html")),
        ];
        candidates.into_iter().find(|s| match s {
            Source::Page(p) => p.is_file() && !p.to_string_lossy().contains('['),
            Source::Content(p) => p.is_file(),
        })
    }

    // -------------------------------------------------------------- rendering

    pub fn render(&self, source: &Source, url: &str) -> Result<String> {
        match source {
            Source::Page(p) => self.render_page(p, url),
            Source::Content(p) => self.render_content(p, url),
        }
    }

    fn base_scope<'a>(&'a self, origin: &str, url: &str, source: &Path) -> Scope<'a> {
        let mut scope = Scope::new(self, origin);
        scope.set("env", Value::Object(self.env.clone()));
        scope.set("site", self.config.site.clone());
        scope.set(
            "page",
            json!({"url": url, "path": self.rel(source), "mode": if self.mode == Mode::Dev {"dev"} else {"build"}}),
        );
        scope
    }

    pub fn render_page(&self, path: &Path, url: &str) -> Result<String> {
        let origin = self.rel(path);
        self.validate(path)?;
        let html = Composer { root: &self.root }.compose_page(path)?;
        let html = self.shortcodes(&html);
        let nodes = template::parse(&html, &origin)?;
        let mut scope = self.base_scope(&origin, url, path);
        let mut out = String::with_capacity(html.len());
        template::render(&nodes, &mut scope, &mut out)?;
        Ok(self.finish(out))
    }

    pub fn render_content(&self, md_path: &Path, url: &str) -> Result<String> {
        let origin = self.rel(md_path);
        let src =
            std::fs::read_to_string(md_path).with_context(|| format!("cannot read {origin}"))?;
        let doc = markdown::parse_front_matter(&src, &origin)?;
        let rendered = markdown::render(&doc.body);
        let composer = Composer { root: &self.root };
        let content_html = composer.resolve_includes_lenient(&rendered.html, &origin)?;

        let template_path = self.page_for_content(md_path).with_context(|| {
            format!(
                "{origin}: no page template found. Create `pages/{}.html` or a `[content].html` in the matching pages folder.",
                to_url_path(&md_path.strip_prefix(self.root.join("content")).unwrap_or(md_path).with_extension(""))
            )
        })?;
        let tpl_origin = self.rel(&template_path);
        self.validate(&template_path)?;
        let fm = Value::Object(doc.front_matter);

        let mut html = composer.compose_page(&template_path)?;
        if html.contains("{static_content_element}") {
            let el = format!(
                "<div id=\"static-content\" style=\"display:none;\" data-toc=\"{}\" data-frontmatter=\"{}\"></div>",
                attr_escape(&rendered.toc.to_string()),
                attr_escape(&fm.to_string())
            );
            html = html.replace("{static_content_element}", &el);
        }
        let html = self.shortcodes(&html);
        let nodes = template::parse(&html, &tpl_origin)?;

        let mut scope = self.base_scope(&tpl_origin, url, md_path);
        scope.set("frontmatter", fm.clone());
        scope.set("toc", rendered.toc.clone());
        scope.set("content", Value::String(content_html));
        let mut out = String::with_capacity(html.len() * 2);
        template::render(&nodes, &mut scope, &mut out)?;

        // Same globals the original exposed, but emitted as safe JS literals.
        let script = format!(
            "<script>window.toc = {}; window.frontmatter = {};</script>\n",
            script_json(&rendered.toc),
            script_json(&fm)
        );
        out = insert_before(&out, "</head>", &script);
        Ok(self.finish(out))
    }

    /// Check `<ForEach>`/`<If>` nesting in the raw file first, so errors point at
    /// the line in *your* file rather than in the composed page.
    fn validate(&self, path: &Path) -> Result<()> {
        if let Ok(src) = std::fs::read_to_string(path) {
            template::parse(&src, &self.rel(path))?;
        }
        Ok(())
    }

    /// Find the page template for a markdown file (same search as the original):
    /// `pages/<same path>.html`, then walking up: `[content].html`, `index.html`,
    /// `<dir>.html` in the parent.
    pub fn page_for_content(&self, md: &Path) -> Option<PathBuf> {
        let content = self.root.join("content");
        let pages = self.root.join("pages");
        let rel = md.strip_prefix(&content).ok()?;
        let direct = pages.join(rel).with_extension("html");
        if direct.is_file() && md.file_stem().is_some_and(|s| s != "index") {
            return Some(direct);
        }
        let mut dir = pages.join(rel.parent()?);
        loop {
            for cand in [dir.join("[content].html"), dir.join("index.html")] {
                if cand.is_file() {
                    return Some(cand);
                }
            }
            if dir == pages {
                return None;
            }
            let parent = dir.parent()?.to_path_buf();
            let sibling = parent.join(format!("{}.html", dir.file_name()?.to_string_lossy()));
            if sibling.is_file() {
                return Some(sibling);
            }
            dir = parent;
        }
    }

    fn finish(&self, html: String) -> String {
        parse_urls(&html, &self.base_url)
    }

    /// `{tailwindcss}`
    fn shortcodes(&self, html: &str) -> String {
        if !html.contains("{tailwindcss}") {
            return html.to_string();
        }
        let asset_base = if self.base_url == "relative" {
            ""
        } else {
            self.base_url.as_str()
        };
        let compiled = self.css_compiled.load(std::sync::atomic::Ordering::Relaxed);
        let replacement = if self.mode == Mode::Build && compiled {
            format!("<link href=\"{asset_base}/assets/css/main.css\" rel=\"stylesheet\">")
        } else {
            self.tailwind_cdn()
        };
        html.replacen("{tailwindcss}", &replacement, 1)
    }

    /// In-browser Tailwind for dev (and as a build fallback when no compiler is available).
    pub fn tailwind_cdn(&self) -> String {
        let css =
            std::fs::read_to_string(self.root.join("assets/css/main.css")).unwrap_or_default();
        match self.tailwind {
            Tailwind::V3 => {
                let css: String = css
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("@tailwind"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut s = String::from(
                    "<script src=\"https://cdn.tailwindcss.com/3.4.17?plugins=forms,typography,aspect-ratio,line-clamp,container-queries\"></script>",
                );
                if let Some(cfg) = inline_tailwind_config(&self.root) {
                    s.push_str(&format!("<script>tailwind.config = {cfg};</script>"));
                }
                s.push_str(&format!("<style type=\"text/tailwindcss\">{css}</style>"));
                s
            }
            Tailwind::V4 | Tailwind::None => {
                let css: String = css
                    .lines()
                    .filter(|l| {
                        let t = l.trim_start();
                        !(t.starts_with("@import \"tailwindcss")
                            || t.starts_with("@import 'tailwindcss")
                            || t.starts_with("@plugin")
                            || t.starts_with("@config")
                            || t.starts_with("@source")
                            || t.starts_with("@tailwind"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "<script src=\"https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4\"></script><style type=\"text/tailwindcss\">{css}</style>"
                )
            }
        }
    }

    // ------------------------------------------------------------------ data

    fn load_content_collection(&self, dir: &str) -> Result<Value> {
        let path = self.root.join("content").join(dir.trim_matches('/'));
        if !path.is_dir() {
            bail!("content folder `{}` does not exist", self.rel(&path));
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("md"))
            .collect();
        files.sort();
        let mut items = Vec::new();
        for f in files {
            let origin = self.rel(&f);
            let src = std::fs::read_to_string(&f)?;
            let doc = markdown::parse_front_matter(&src, &origin)?;
            let mut item = doc.front_matter;
            if self.mode == Mode::Build && item.get("draft").is_some_and(template::expr::truthy) {
                continue;
            }
            let rel = to_url_path(
                &f.strip_prefix(self.root.join("content"))?
                    .with_extension(""),
            );
            let slug = f.file_stem().unwrap().to_string_lossy().to_string();
            item.insert(
                "link".into(),
                Value::String(format!(
                    "/{}",
                    rel.trim_end_matches("/index").trim_end_matches("index")
                )),
            );
            item.insert("slug".into(), Value::String(slug));
            item.insert(
                "html".into(),
                Value::String(markdown::render(&doc.body).html),
            );
            item.insert("content".into(), Value::String(doc.body));
            items.push(Value::Object(item));
        }
        // Original behaviour: sort by `date` (ascending) when present.
        if items.iter().any(|i| i.get("date").is_some()) {
            template::order_by(&mut items, "date");
        }
        Ok(Value::Array(items))
    }
}

impl DataSource for Site {
    fn collection(&self, name: &str) -> Result<Value> {
        self.cached(&format!("collection:{name}"), || {
            let name = name.trim_matches('/');
            if let Some(dir) = name.strip_prefix("content/") {
                return self.load_content_collection(dir);
            }
            let p = self.root.join("collections").join(format!("{name}.json"));
            let text = std::fs::read_to_string(&p)
                .with_context(|| format!("cannot read collection `{}`", self.rel(&p)))?;
            serde_json::from_str(&text)
                .with_context(|| format!("`{}` is not valid JSON", self.rel(&p)))
        })
    }

    fn content_collection(&self, dir: &str) -> Result<Value> {
        self.cached(&format!("content:{dir}"), || {
            self.load_content_collection(dir)
        })
    }
}

// ------------------------------------------------------------------ helpers

pub fn detect_tailwind(root: &Path) -> Tailwind {
    match std::fs::read_to_string(root.join("assets/css/main.css")) {
        Err(_) => Tailwind::None,
        Ok(css)
            if css
                .lines()
                .any(|l| l.trim_start().starts_with("@tailwind ")) =>
        {
            Tailwind::V3
        }
        Ok(_) => Tailwind::V4,
    }
}

/// Extract `module.exports = {...}` from tailwind.config.js for the Play CDN,
/// minus `plugins` (the CDN loads the official ones via query string).
fn inline_tailwind_config(root: &Path) -> Option<String> {
    static PLUGINS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)plugins\s*:\s*\[.*?\]\s*\]?\s*,?").unwrap());
    let src = std::fs::read_to_string(root.join("tailwind.config.js")).ok()?;
    let start = src.find("module.exports")?;
    let body = src[start..].split_once('=')?.1.trim();
    let body = PLUGINS
        .replace_all(body, "")
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string();
    if body.contains("require(") || !body.starts_with('{') {
        return None;
    }
    Some(body)
}

/// `{ url('/path') }` → `/path` or `https://example.com/path`.
pub fn parse_urls(html: &str, base: &str) -> String {
    static URL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\{\s*url\(\s*['"]([^'"]*)['"]\s*\)\s*\}"#).unwrap());
    if !html.contains("url(") {
        return html.to_string();
    }
    URL.replace_all(html, |c: &regex::Captures| {
        if base == "relative" {
            c[1].to_string()
        } else {
            format!("{}{}", base, &c[1])
        }
    })
    .into_owned()
}

fn to_url_path(p: &Path) -> String {
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `about` → (`/about/`, `about/index.html`); `blog/index` → (`/blog/`, `blog/index.html`).
fn route_for(rel: &str) -> (String, PathBuf) {
    let dir = rel
        .strip_suffix("index")
        .map(|d| d.trim_end_matches('/'))
        .unwrap_or(rel);
    let dir = if rel == "index" { "" } else { dir };
    let url = if dir.is_empty() {
        "/".to_string()
    } else {
        format!("/{dir}/")
    };
    let out = if dir.is_empty() {
        PathBuf::from("index.html")
    } else {
        PathBuf::from(dir).join("index.html")
    };
    (url, out)
}

/// Normalise a request path; rejects `..` and other traversal attempts.
pub fn sanitize_url_path(url_path: &str) -> Option<String> {
    let path = url_path.split(['?', '#']).next().unwrap_or("");
    let decoded = percent_decode(path);
    let mut parts = Vec::new();
    for seg in decoded.split('/') {
        match seg {
            "" | "." => {}
            ".." => return None,
            s if s.contains('\\') || s.contains('\0') => return None,
            s => parts.push(s),
        }
    }
    let mut s = parts.join("/");
    for ext in [".html", ".htm"] {
        if let Some(stripped) = s.strip_suffix(ext) {
            s = stripped.to_string();
        }
    }
    if s == "index" || s.ends_with("/index") {
        s.truncate(s.len() - "index".len());
        s = s.trim_end_matches('/').to_string();
    }
    Some(s)
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// JSON that is safe inside a `<script>` element.
fn script_json(v: &Value) -> String {
    v.to_string()
        .replace("</", "<\\/")
        .replace("<!--", "<\\!--")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

pub fn insert_before(html: &str, needle: &str, insert: &str) -> String {
    match html.rfind(needle) {
        Some(i) => format!("{}{}{}", &html[..i], insert, &html[i..]),
        None => format!("{html}{insert}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes() {
        assert_eq!(
            route_for("index"),
            ("/".into(), PathBuf::from("index.html"))
        );
        assert_eq!(
            route_for("about"),
            ("/about/".into(), PathBuf::from("about/index.html"))
        );
        assert_eq!(
            route_for("blog/index"),
            ("/blog/".into(), PathBuf::from("blog/index.html"))
        );
        assert_eq!(
            route_for("blog/post"),
            ("/blog/post/".into(), PathBuf::from("blog/post/index.html"))
        );
    }

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_url_path("/").as_deref(), Some(""));
        assert_eq!(sanitize_url_path("/about/").as_deref(), Some("about"));
        assert_eq!(
            sanitize_url_path("/about.html?x=1").as_deref(),
            Some("about")
        );
        assert_eq!(
            sanitize_url_path("/blog/index.html").as_deref(),
            Some("blog")
        );
        assert_eq!(sanitize_url_path("/%2e%2e/etc/passwd"), None);
        assert_eq!(sanitize_url_path("/../x"), None);
    }

    #[test]
    fn urls() {
        assert_eq!(
            parse_urls("<a href=\"{ url('/a') }\">", "relative"),
            "<a href=\"/a\">"
        );
        assert_eq!(
            parse_urls("{url(\"/a\")}", "https://x.com"),
            "https://x.com/a"
        );
    }

    #[test]
    fn script_json_escapes() {
        assert_eq!(
            script_json(&json!({"a": "</script>"})),
            r#"{"a":"<\/script>"}"#
        );
    }
}

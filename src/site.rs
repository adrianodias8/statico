//! A Statico project on disk: routing, data loading and page rendering.
//! Shared by `statico dev` (render on request) and `statico build`.

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use walkdir::WalkDir;

use crate::config::{Config, PUBLIC_ENV_PREFIX, load_env, public_env};
use crate::markdown;
use crate::template::{self, DataSource, Scope, compose::Composer, filters};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dev,
    Build,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tailwind {
    /// `@tailwind base;` + `tailwind.config.js` (all official Static templates).
    V3,
    /// `@import "tailwindcss";`
    V4,
    /// No `assets/css/main.css`, or plain CSS with no Tailwind directive.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Page(PathBuf),
    Content(PathBuf),
    /// One page per taxonomy term: `pages/tags/[tag].html` rendered for `slug`.
    Term {
        template: PathBuf,
        var: String,
        slug: String,
    },
}

impl Source {
    pub fn path(&self) -> &Path {
        match self {
            Source::Page(p) | Source::Content(p) => p,
            Source::Term { template, .. } => template,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Route {
    /// URL path, e.g. `/blog/hello/`.
    pub url: String,
    /// Output file relative to the build dir, e.g. `blog/hello/index.html`.
    pub out: PathBuf,
    pub source: Source,
}

/// A rendered page plus the metadata feeds, llms.txt, search and SEO use.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub html: String,
    pub title: String,
    pub description: String,
    /// Total pages when the page uses `<ForEach paginate>` (1 otherwise).
    pub total_pages: usize,
    /// Markdown source (content pages only), for `.md` mirrors.
    pub markdown: Option<String>,
    pub date: Option<String>,
}

/// A taxonomy term (`tags: [rust, web]` → `rust`, `web`).
#[derive(Debug, Clone)]
pub struct Term {
    pub name: String,
    pub slug: String,
    pub count: usize,
}

type CacheCell = Arc<OnceLock<Result<Value, String>>>;

pub struct Site {
    pub root: PathBuf,
    pub config: Config,
    pub mode: Mode,
    /// `relative` or an absolute URL without trailing slash.
    pub base_url: String,
    pub tailwind: Tailwind,
    /// Set by the build once main.css has been compiled.
    pub css_compiled: std::sync::atomic::AtomicBool,
    /// The `PUBLIC_` subset of `.env`; the only part templates can read.
    env: Map<String, Value>,
    /// Keys in `.env` that templates can't read, kept to explain a `{env.KEY}`
    /// that stays unresolved.
    private_env_keys: Vec<String>,
    /// Single-flight cache: parallel pages needing the same collection wait
    /// for one computation instead of all doing it.
    cache: Mutex<HashMap<String, CacheCell>>,
    md_cache: Mutex<HashMap<u64, Arc<markdown::Rendered>>>,
    highlight: Option<String>,
}

impl Site {
    pub fn load(root: &Path, mode: Mode, url_override: Option<String>) -> Result<Self> {
        let root = root
            .canonicalize()
            .with_context(|| format!("cannot open project at {}", root.display()))?;
        if !root.join("pages").is_dir() {
            bail!(
                "no `pages` directory in {}\n  Run this inside a Statico project, or create one with `statico new my-site` (or `statico init`).",
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
        let highlight = config.markdown.theme();
        if let Some(t) = &highlight
            && !markdown::theme_names().contains(t)
        {
            bail!(
                "unknown code highlighting theme `{t}` in statico.json. Available: {}",
                markdown::theme_names().join(", ")
            );
        }
        let all_env = load_env(&root);
        let private_env_keys = all_env
            .keys()
            .filter(|k| !k.starts_with(PUBLIC_ENV_PREFIX))
            .cloned()
            .collect();
        Ok(Site {
            tailwind: detect_tailwind(&root),
            env: public_env(&all_env),
            private_env_keys,
            root,
            config,
            mode,
            base_url,
            css_compiled: false.into(),
            cache: Mutex::new(HashMap::new()),
            md_cache: Mutex::new(HashMap::new()),
            highlight,
        })
    }

    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
        self.md_cache.lock().unwrap().clear();
    }

    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root)
            .unwrap_or(p)
            .display()
            .to_string()
            .replace('\\', "/")
    }

    /// `https://example.com/x` for absolute builds, `/x` otherwise.
    pub fn abs_url(&self, path: &str) -> String {
        if self.base_url == "relative"
            || path.starts_with("http://")
            || path.starts_with("https://")
        {
            path.to_string()
        } else {
            format!("{}{}", self.base_url, path)
        }
    }

    pub fn site_str(&self, key: &str) -> Option<String> {
        self.config
            .site
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    fn cached(&self, key: &str, f: impl FnOnce() -> Result<Value>) -> Result<Value> {
        let cell = self
            .cache
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_default()
            .clone();
        cell.get_or_init(|| f().map_err(|e| format!("{e:#}")))
            .clone()
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Markdown → HTML, memoised by content (collections and pages share it).
    pub fn render_markdown(&self, md: &str) -> Arc<markdown::Rendered> {
        let key = crate::postprocess::fnv1a(md.as_bytes());
        if let Some(r) = self.md_cache.lock().unwrap().get(&key) {
            return r.clone();
        }
        let r = Arc::new(markdown::render_with(md, self.highlight.as_deref()));
        self.md_cache.lock().unwrap().insert(key, r.clone());
        r
    }

    // ---------------------------------------------------------------- routing

    /// Every page the build should write (before pagination expands them).
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
                continue; // `[content].html`, `[tag].html` are templates
            }
            let rel = to_url_path(&p.strip_prefix(&pages)?.with_extension(""));
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

        for p in self.content_files()? {
            if self.mode == Mode::Build && self.is_draft(&p)? {
                continue;
            }
            let rel = to_url_path(
                &p.strip_prefix(self.root.join("content"))?
                    .with_extension(""),
            );
            let (url, out) = route_for(&rel);
            // Content wins over a page with the same URL (as in `statico dev`).
            if let Some(existing) = routes.iter().position(|r| r.url == url) {
                crate::diag::warn(format!(
                    "{}: maps to {url} like {}; using the markdown file",
                    self.rel(&p),
                    self.rel(routes[existing].source.path()),
                ));
                routes.remove(existing);
            }
            routes.push(Route {
                url,
                out,
                source: Source::Content(p),
            });
        }

        for (template, var, dir_url) in self.term_templates()? {
            for term in self.terms(&var)? {
                let url = format!("{dir_url}{}/", term.slug);
                routes.push(Route {
                    out: PathBuf::from(url.trim_start_matches('/')).join("index.html"),
                    url,
                    source: Source::Term {
                        template: template.clone(),
                        var: var.clone(),
                        slug: term.slug,
                    },
                });
            }
        }
        Ok(routes)
    }

    pub fn content_files(&self) -> Result<Vec<PathBuf>> {
        let content = self.root.join("content");
        let mut files = Vec::new();
        if content.is_dir() {
            for e in WalkDir::new(&content).sort_by_file_name() {
                let e = e?;
                if e.file_type().is_file()
                    && e.path().extension().and_then(|x| x.to_str()) == Some("md")
                {
                    files.push(e.path().to_path_buf());
                }
            }
        }
        Ok(files)
    }

    fn front_matter(&self, md: &Path) -> Result<Map<String, Value>> {
        let src = std::fs::read_to_string(md)?;
        Ok(markdown::parse_front_matter(&src, &self.rel(md))?.front_matter)
    }

    fn is_draft(&self, md: &Path) -> Result<bool> {
        Ok(self
            .front_matter(md)?
            .get("draft")
            .is_some_and(template::expr::truthy))
    }

    /// `pages/**/[name].html` (other than `[content]`): (template, var, URL dir).
    pub fn term_templates(&self) -> Result<Vec<(PathBuf, String, String)>> {
        let pages = self.root.join("pages");
        let mut out = Vec::new();
        for e in WalkDir::new(&pages).sort_by_file_name() {
            let e = e?;
            let name = e.file_name().to_string_lossy().to_string();
            let Some(var) = name
                .strip_prefix('[')
                .and_then(|n| n.strip_suffix("].html"))
            else {
                continue;
            };
            if var == "content" || var.is_empty() {
                continue;
            }
            let dir = to_url_path(e.path().parent().unwrap().strip_prefix(&pages)?);
            let dir_url = if dir.is_empty() {
                "/".into()
            } else {
                format!("/{dir}/")
            };
            out.push((e.path().to_path_buf(), var.to_string(), dir_url));
        }
        Ok(out)
    }

    /// All terms for a taxonomy across `content/`. `name` may be singular or
    /// plural: `tag`/`tags`, `category`/`categories`.
    pub fn terms(&self, name: &str) -> Result<Vec<Term>> {
        let v = self.cached(&format!("terms:{name}"), || {
            let keys = taxonomy_keys(name);
            let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
            for f in self.content_files()? {
                let fm = self.front_matter(&f)?;
                if self.mode == Mode::Build && fm.get("draft").is_some_and(template::expr::truthy) {
                    continue;
                }
                for key in &keys {
                    let Some(v) = fm.get(key) else { continue };
                    let names: Vec<String> = match v {
                        Value::Array(a) => a.iter().map(template::value_to_string).collect(),
                        Value::String(s) => vec![s.clone()],
                        _ => vec![],
                    };
                    for n in names.into_iter().filter(|n| !n.trim().is_empty()) {
                        let slug = filters::slugify(&n);
                        counts.entry(slug).or_insert((n, 0)).1 += 1;
                    }
                    break;
                }
            }
            let mut terms: Vec<Value> = counts
                .into_iter()
                .map(|(slug, (name, count))| json!({"name": name, "slug": slug, "count": count}))
                .collect();
            // Most used first, then alphabetical.
            terms.sort_by(|a, b| {
                b["count"]
                    .as_u64()
                    .cmp(&a["count"].as_u64())
                    .then_with(|| a["slug"].as_str().cmp(&b["slug"].as_str()))
            });
            Ok(Value::Array(terms))
        })?;
        Ok(v.as_array()
            .unwrap()
            .iter()
            .map(|t| Term {
                name: t["name"].as_str().unwrap_or_default().to_string(),
                slug: t["slug"].as_str().unwrap_or_default().to_string(),
                count: t["count"].as_u64().unwrap_or(0) as usize,
            })
            .collect())
    }

    fn term_url(&self, var: &str, slug: &str) -> Option<String> {
        let names = taxonomy_keys(var);
        self.term_templates()
            .ok()?
            .into_iter()
            .find(|(_, v, _)| names.contains(v) || taxonomy_keys(v).contains(&var.to_string()))
            .map(|(_, _, dir)| format!("{dir}{slug}/"))
    }

    /// Map a request path to a source (dev server). Returns the page number
    /// for `/blog/page/2/`.
    pub fn resolve(&self, url_path: &str) -> Option<(Source, usize)> {
        let clean = sanitize_url_path(url_path)?;
        if let Some(src) = self.resolve_plain(&clean) {
            return Some((src, 1));
        }
        // `/blog/page/3` → `/blog` page 3
        let parts: Vec<&str> = clean.split('/').collect();
        if parts.len() >= 2
            && parts[parts.len() - 2] == "page"
            && let Ok(n) = parts[parts.len() - 1].parse::<usize>()
            && n >= 1
        {
            let base = parts[..parts.len() - 2].join("/");
            if let Some(src) = self.resolve_plain(&base) {
                return Some((src, n));
            }
        }
        None
    }

    fn resolve_plain(&self, clean: &str) -> Option<Source> {
        let route = if clean.is_empty() { "index" } else { clean };
        let content = self.root.join("content");
        let pages = self.root.join("pages");
        let candidates = [
            Source::Content(content.join(format!("{route}.md"))),
            Source::Content(content.join(route).join("index.md")),
            Source::Page(pages.join(format!("{route}.html"))),
            Source::Page(pages.join(route).join("index.html")),
        ];
        if let Some(s) = candidates.into_iter().find(|s| {
            let p = s.path();
            p.is_file() && !p.file_name().unwrap().to_string_lossy().contains('[')
        }) {
            return Some(s);
        }
        // Taxonomy term: `/tags/rust` with `pages/tags/[tag].html`.
        let (dir, slug) = match clean.rsplit_once('/') {
            Some((d, s)) => (format!("/{d}/"), s),
            None => ("/".to_string(), clean),
        };
        for (template, var, dir_url) in self.term_templates().ok()? {
            if dir_url == dir && self.terms(&var).ok()?.iter().any(|t| t.slug == slug) {
                return Some(Source::Term {
                    template,
                    var,
                    slug: slug.to_string(),
                });
            }
        }
        None
    }

    // -------------------------------------------------------------- rendering

    /// Render page `page` (1-based) of a source at `url` (its first-page URL).
    pub fn render(&self, source: &Source, url: &str, page: usize) -> Result<Rendered> {
        let mut r = self.render_pass(source, url, page, None)?;
        if r.total_pages > 1 || page > 1 {
            if page > r.total_pages {
                bail!(
                    "{}: page {page} does not exist (only {})",
                    self.rel(source.path()),
                    r.total_pages
                );
            }
            // Second pass with `{pagination.*}` filled in.
            r = self.render_pass(source, url, page, Some(r.total_pages))?;
        }
        Ok(r)
    }

    /// Private `.env` keys that `html` still has as `{env.KEY}` placeholders.
    pub fn private_env_refs<'a>(&'a self, html: &str) -> Vec<&'a str> {
        self.private_env_keys
            .iter()
            .filter(|k| {
                html.contains(&format!("{{env.{k}}}")) || html.contains(&format!("{{env.{k}|"))
            })
            .map(String::as_str)
            .collect()
    }

    fn base_scope<'a>(&'a self, url: &str, source: &Path, page: usize) -> Scope<'a> {
        let mut scope = Scope::new(self);
        scope.base_url = self.base_url.clone();
        scope.page_number = page;
        scope.set("env", Value::Object(self.env.clone()));
        scope.set("site", self.config.site.clone());
        scope.set(
            "page",
            json!({
                "url": page_url(url, page),
                "path": self.rel(source),
                "mode": if self.mode == Mode::Dev {"dev"} else {"build"},
            }),
        );
        scope
    }

    fn render_pass(
        &self,
        source: &Source,
        url: &str,
        page: usize,
        total: Option<usize>,
    ) -> Result<Rendered> {
        let composer = Composer { root: &self.root };
        let (tpl_path, md) = match source {
            Source::Page(p) => (p.clone(), None),
            Source::Term { template, .. } => (template.clone(), None),
            Source::Content(md_path) => {
                let tpl = self.page_for_content(md_path).with_context(|| {
                    format!(
                        "{}: no page template found. Create `pages/{}.html` or a `[content].html` in the matching pages folder.",
                        self.rel(md_path),
                        to_url_path(&md_path.strip_prefix(self.root.join("content")).unwrap_or(md_path).with_extension(""))
                    )
                })?;
                (tpl, Some(md_path.clone()))
            }
        };
        let origin = self.rel(&tpl_path);
        self.validate(&tpl_path)?;
        let (mut html, layout_attrs) = composer.compose_page_with_attrs(&tpl_path)?;
        let mut scope = self.base_scope(url, source.path(), page);

        let mut fm = Value::Null;
        let mut md_source = None;
        let mut toc = Value::Array(vec![]);
        if let Some(md_path) = &md {
            let md_origin = self.rel(md_path);
            let src = std::fs::read_to_string(md_path)
                .with_context(|| format!("cannot read {md_origin}"))?;
            let doc = markdown::parse_front_matter(&src, &md_origin)?;
            self.check_schema(md_path, &doc.front_matter)?;
            let rendered = self.render_markdown(&doc.body);
            let content_html = composer.resolve_includes_lenient(&rendered.html, &md_origin)?;
            fm = Value::Object(doc.front_matter);
            toc = rendered.toc.clone();
            if html.contains("{static_content_element}") {
                let el = format!(
                    "<div id=\"static-content\" style=\"display:none;\" data-toc=\"{}\" data-frontmatter=\"{}\"></div>",
                    attr_escape(&toc.to_string()),
                    attr_escape(&fm.to_string())
                );
                html = html.replace("{static_content_element}", &el);
            }
            scope.set("frontmatter", fm.clone());
            scope.set("toc", toc.clone());
            scope.set("content", Value::String(content_html));
            md_source = Some(doc.body);
        }
        if let Source::Term { var, slug, .. } = source {
            let term = self
                .terms(var)?
                .into_iter()
                .find(|t| &t.slug == slug)
                .with_context(|| format!("{origin}: no `{var}` called `{slug}`"))?;
            scope.set(var.clone(), Value::String(term.name.clone()));
            scope.set(
                "term",
                json!({"name": term.name, "slug": term.slug, "count": term.count, "url": url}),
            );
        }
        if let Some(total) = total {
            scope.set("pagination", pagination_value(url, page, total));
        }

        // Title & description for {page.title}, {seo}, feeds and llms.txt.
        let raw_title = layout_attrs
            .iter()
            .find(|(k, _)| k == "title")
            .map(|(_, v)| v.clone())
            .or_else(|| fm.get("title").map(template::value_to_string))
            .unwrap_or_default();
        let title = template::substitute(&raw_title, &scope);
        let description = ["description", "excerpt", "summary"]
            .iter()
            .find_map(|k| fm.get(*k).map(template::value_to_string))
            .or_else(|| {
                layout_attrs
                    .iter()
                    .find(|(k, _)| k == "description")
                    .map(|(_, v)| template::substitute(v, &scope))
            })
            .or_else(|| self.site_str("description"))
            .unwrap_or_default();
        scope.set("page.title", Value::String(title.clone()));
        scope.set("page.description", Value::String(description.clone()));
        if html.contains("{seo}") {
            let seo = crate::seo::tags(
                self,
                &crate::seo::PageMeta {
                    title: &title,
                    description: &description,
                    url: &page_url(url, page),
                    front_matter: &fm,
                    is_article: md.is_some() && fm.get("date").is_some(),
                },
            );
            scope.set("seo", Value::String(seo));
        }

        let html = self.shortcodes(&html);
        let nodes = template::parse(&html, &origin)?;
        let mut out = String::with_capacity(html.len() * 2);
        template::render(&nodes, &mut scope, &mut out)?;
        let total_pages = scope
            .paginated
            .map(|(per, n)| n.div_ceil(per).max(1))
            .unwrap_or(1);

        if md.is_some() {
            // Same globals the original exposed, but emitted as safe JS literals.
            let script = format!(
                "<script>window.toc = {}; window.frontmatter = {};</script>\n",
                script_json(&toc),
                script_json(&fm)
            );
            out = insert_before(&out, "</head>", &script);
        }
        Ok(Rendered {
            html: parse_urls(&out, &self.base_url),
            title,
            description,
            total_pages,
            markdown: md_source,
            date: fm.get("date").map(template::value_to_string),
        })
    }

    /// Check `<ForEach>`/`<If>` nesting in the raw file first, so errors point at
    /// the line in *your* file rather than in the composed page.
    fn validate(&self, path: &Path) -> Result<()> {
        if let Ok(src) = std::fs::read_to_string(path) {
            template::parse(&src, &self.rel(path))?;
        }
        Ok(())
    }

    /// Validate front matter against `schemas` in statico.json.
    pub fn check_schema(&self, md: &Path, fm: &Map<String, Value>) -> Result<()> {
        let Ok(rel) = md.strip_prefix(self.root.join("content")) else {
            return Ok(());
        };
        let folder = to_url_path(rel.parent().unwrap_or(Path::new("")));
        let Some(Value::Object(schema)) = self
            .config
            .schemas
            .iter()
            .filter(|(k, _)| folder == **k || folder.starts_with(&format!("{k}/")))
            .max_by_key(|(k, _)| k.len())
            .map(|(_, v)| v)
        else {
            return Ok(());
        };
        let mut problems = Vec::new();
        for (field, ty) in schema {
            let ty = ty.as_str().unwrap_or("any");
            let (ty, optional) = match ty.strip_suffix('?') {
                Some(t) => (t, true),
                None => (ty, false),
            };
            match fm.get(field) {
                None | Some(Value::Null) if optional => {}
                None | Some(Value::Null) => {
                    problems.push(format!("missing required field `{field}` ({ty})"))
                }
                Some(v) => {
                    let ok = match ty {
                        "string" | "text" => v.is_string(),
                        "number" => v.is_number(),
                        "bool" | "boolean" => v.is_boolean(),
                        "list" | "array" => v.is_array(),
                        "date" => v.as_str().and_then(filters::parse_date).is_some(),
                        "url" => v
                            .as_str()
                            .is_some_and(|s| s.starts_with('/') || s.starts_with("http")),
                        _ => true,
                    };
                    if !ok {
                        problems.push(format!("field `{field}` must be a {ty} (got {v})"));
                    }
                }
            }
        }
        if !problems.is_empty() {
            bail!(
                "{}: front matter does not match the `{folder}` schema in statico.json: {} [S201]",
                self.rel(md),
                problems.join("; ")
            );
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
        let plain = self.tailwind == Tailwind::None;
        let replacement = if plain || (self.mode == Mode::Build && compiled) {
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

    pub fn load_content_collection(&self, dir: &str) -> Result<Value> {
        let path = self.root.join("content").join(dir.trim_matches('/'));
        if !path.is_dir() {
            bail!("content folder `{}` does not exist", self.rel(&path));
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("md"))
            .collect();
        files.sort();
        use rayon::prelude::*;
        let loaded: Vec<Result<Option<Value>>> = off_the_page_pool(|| {
            files
                .par_iter()
                .map(|f| {
                    let origin = self.rel(f);
                    let src = std::fs::read_to_string(f)?;
                    let doc = markdown::parse_front_matter(&src, &origin)?;
                    let mut item = doc.front_matter;
                    if self.mode == Mode::Build
                        && item.get("draft").is_some_and(template::expr::truthy)
                    {
                        return Ok(None);
                    }
                    let rel = to_url_path(
                        &f.strip_prefix(self.root.join("content"))?
                            .with_extension(""),
                    );
                    let slug = f.file_stem().unwrap().to_string_lossy().to_string();
                    let link = format!(
                        "/{}",
                        rel.trim_end_matches("/index").trim_end_matches("index")
                    );
                    let rendered = self.render_markdown(&doc.body);
                    item.insert("link".into(), Value::String(link.clone()));
                    item.insert(
                        "url".into(),
                        Value::String(format!("{}/", link.trim_end_matches('/'))),
                    );
                    item.insert("slug".into(), Value::String(slug));
                    // Same as on the post's own page, so `{post.html}` in a
                    // listing and the feed don't publish raw `<include>` tags.
                    let html = Composer { root: &self.root }
                        .resolve_includes_lenient(&rendered.html, &origin)?;
                    item.insert("html".into(), Value::String(html));
                    item.insert("content".into(), Value::String(doc.body));
                    Ok(Some(Value::Object(item)))
                })
                .collect()
        });
        let mut items = Vec::with_capacity(loaded.len());
        for r in loaded {
            if let Some(v) = r? {
                items.push(v);
            }
        }
        // Original behaviour: sort by `date` (ascending) when present.
        if items.iter().any(|i| i.get("date").is_some()) {
            template::order_by(&mut items, "date");
        }
        Ok(Value::Array(items))
    }

    /// `collections/<name>.{json,yaml,yml,toml,csv}`
    fn load_data_file(&self, name: &str) -> Result<Value> {
        let dir = self.root.join("collections");
        for ext in ["json", "yaml", "yml", "toml", "csv"] {
            let p = dir.join(format!("{name}.{ext}"));
            if !p.is_file() {
                continue;
            }
            let rel = self.rel(&p);
            let text =
                std::fs::read_to_string(&p).with_context(|| format!("cannot read `{rel}`"))?;
            let v: Value = match ext {
                "json" => serde_json::from_str(&text)
                    .with_context(|| format!("`{rel}` is not valid JSON"))?,
                "yaml" | "yml" => serde_yaml::from_str(&text)
                    .with_context(|| format!("`{rel}` is not valid YAML"))?,
                "toml" => {
                    let t: toml::Table = toml::from_str(&text)
                        .with_context(|| format!("`{rel}` is not valid TOML"))?;
                    let v = serde_json::to_value(t)?;
                    // A TOML file with a single array-of-tables is a list.
                    match v.as_object() {
                        Some(m)
                            if m.len() == 1 && m.values().next().is_some_and(Value::is_array) =>
                        {
                            m.values().next().unwrap().clone()
                        }
                        _ => v,
                    }
                }
                _ => {
                    let mut rdr = csv::Reader::from_reader(text.as_bytes());
                    let headers = rdr.headers()?.clone();
                    let mut rows = Vec::new();
                    for rec in rdr.records() {
                        let rec = rec.with_context(|| format!("`{rel}` is not valid CSV"))?;
                        let mut m = Map::new();
                        for (h, v) in headers.iter().zip(rec.iter()) {
                            let val = v
                                .parse::<i64>()
                                .map(Value::from)
                                .or_else(|_| v.parse::<f64>().map(Value::from))
                                .unwrap_or_else(|_| Value::String(v.to_string()));
                            m.insert(h.to_string(), val);
                        }
                        rows.push(Value::Object(m));
                    }
                    Value::Array(rows)
                }
            };
            return Ok(v);
        }
        bail!("no collection `{name}`: create collections/{name}.json (or .yaml, .toml, .csv)")
    }
}

impl DataSource for Site {
    fn collection(&self, name: &str) -> Result<Value> {
        self.cached(&format!("collection:{name}"), || {
            let name = name.trim_matches('/');
            if let Some(dir) = name.strip_prefix("content/") {
                return self.load_content_collection(dir);
            }
            // `taxonomy/tags` → [{name, slug, count, url}]
            if let Some(tax) = name.strip_prefix("taxonomy/") {
                let terms = self.terms(tax)?;
                return Ok(Value::Array(
                    terms
                        .into_iter()
                        .map(|t| {
                            let url = self.term_url(tax, &t.slug).unwrap_or_default();
                            json!({"name": t.name, "slug": t.slug, "count": t.count, "url": url})
                        })
                        .collect(),
                ));
            }
            self.load_data_file(name)
        })
    }

    fn content_collection(&self, dir: &str) -> Result<Value> {
        self.cached(&format!("content:{dir}"), || {
            self.load_content_collection(dir)
        })
    }
}

// ------------------------------------------------------------------ helpers

/// Threads for loading a content folder in parallel. `cached` holds a
/// collection's `OnceLock` while it loads, and a rayon worker waiting on a
/// `par_iter` steals other jobs meanwhile: if it stole a page that needs the
/// same collection, it would wait on its own initialisation forever.
static LOADER: LazyLock<rayon::ThreadPool> = LazyLock::new(|| {
    rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("statico-load-{i}"))
        .build()
        .expect("cannot start content loader threads")
});

/// Run `op` on [`LOADER`] and block until it finishes. The hop through a plain
/// thread matters: a page worker calling `LOADER.install` directly would still
/// steal page jobs while it waits.
fn off_the_page_pool<T: Send>(op: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| {
        s.spawn(|| LOADER.install(op))
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e))
    })
}

/// Front matter keys for a taxonomy name: `tag` → [tags, tag].
fn taxonomy_keys(name: &str) -> Vec<String> {
    let mut v = vec![name.to_string()];
    if let Some(stem) = name.strip_suffix("ies") {
        v.push(format!("{stem}y"));
    } else if let Some(stem) = name.strip_suffix('y') {
        v.insert(0, format!("{stem}ies"));
    } else if let Some(stem) = name.strip_suffix('s') {
        v.push(stem.to_string());
    } else {
        v.insert(0, format!("{name}s"));
    }
    v
}

/// URL of page `n` of a paginated route: `/blog/` → `/blog/page/2/`.
pub fn page_url(base: &str, n: usize) -> String {
    if n <= 1 {
        base.to_string()
    } else {
        format!("{}/page/{n}/", base.trim_end_matches('/'))
    }
}

fn pagination_value(url: &str, page: usize, total: usize) -> Value {
    let pages: Vec<Value> = (1..=total)
        .map(|n| json!({"number": n, "url": page_url(url, n), "current": n == page}))
        .collect();
    json!({
        "current": page,
        "total": total,
        "has_prev": page > 1,
        "has_next": page < total,
        "prev_url": if page > 1 { page_url(url, page - 1) } else { String::new() },
        "next_url": if page < total { page_url(url, page + 1) } else { String::new() },
        "first_url": page_url(url, 1),
        "last_url": page_url(url, total),
        "pages": pages,
    })
}

/// Which Tailwind `assets/css/main.css` is written for. A stylesheet with no
/// Tailwind directive is plain CSS: it is copied as written, never compiled,
/// so a site that doesn't use Tailwind never downloads it.
pub fn detect_tailwind(root: &Path) -> Tailwind {
    let Ok(css) = std::fs::read_to_string(root.join("assets/css/main.css")) else {
        return Tailwind::None;
    };
    let lines = || css.lines().map(str::trim_start);
    if lines().any(|l| l.starts_with("@tailwind ")) {
        return Tailwind::V3;
    }
    const V4: &[&str] = &[
        "@import \"tailwindcss",
        "@import 'tailwindcss",
        "@theme",
        "@plugin",
        "@source",
        "@utility",
        "@custom-variant",
        "@config",
        "@apply",
    ];
    if lines().any(|l| V4.iter().any(|d| l.starts_with(d))) {
        Tailwind::V4
    } else {
        Tailwind::None
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
    let replace = |s: &str| {
        URL.replace_all(s, |c: &regex::Captures| {
            if base == "relative" {
                c[1].to_string()
            } else {
                format!("{}{}", base, &c[1])
            }
        })
        .into_owned()
    };
    // Code shows `{ url('…') }` as written: a page documenting the helper
    // must not have its examples rewritten.
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some((start, close)) = ["<pre", "<code"]
        .iter()
        .filter_map(|t| {
            rest.find(t)
                .map(|i| (i, if *t == "<pre" { "</pre>" } else { "</code>" }))
        })
        .min_by_key(|(i, _)| *i)
    {
        out.push_str(&replace(&rest[..start]));
        let end = rest[start..]
            .find(close)
            .map_or(rest.len(), |e| start + e + close.len());
        out.push_str(&rest[start..end]);
        rest = &rest[end..];
    }
    out.push_str(&replace(rest));
    out
}

pub fn to_url_path(p: &Path) -> String {
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
pub fn script_json(v: &Value) -> String {
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
    fn url_helper_leaves_code_alone() {
        let html = "<a href=\"{ url('/a') }\">a</a> <code>{ url('/b') }</code> <pre><code>{ url('/c') }</code></pre> { url('/d') }";
        assert_eq!(
            parse_urls(html, "https://e.com"),
            "<a href=\"https://e.com/a\">a</a> <code>{ url('/b') }</code> <pre><code>{ url('/c') }</code></pre> https://e.com/d"
        );
    }

    #[test]
    fn tailwind_only_when_the_css_asks_for_it() {
        let d = tempfile::tempdir().unwrap();
        let css = d.path().join("assets/css");
        std::fs::create_dir_all(&css).unwrap();
        let detect = |src: &str| {
            std::fs::write(css.join("main.css"), src).unwrap();
            detect_tailwind(d.path())
        };
        assert_eq!(
            detect(":root { --ink: #111; }\nbody { color: var(--ink); }"),
            Tailwind::None
        );
        assert_eq!(
            detect("@tailwind base;\n@tailwind utilities;"),
            Tailwind::V3
        );
        assert_eq!(
            detect("@import \"tailwindcss\";\n.x { color: red; }"),
            Tailwind::V4
        );
        assert_eq!(detect("@theme {\n  --color-ink: #111;\n}"), Tailwind::V4);
    }

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
    fn pages_and_taxonomy_keys() {
        assert_eq!(page_url("/blog/", 1), "/blog/");
        assert_eq!(page_url("/blog/", 3), "/blog/page/3/");
        assert_eq!(page_url("/", 2), "/page/2/");
        assert_eq!(taxonomy_keys("tag"), vec!["tags", "tag"]);
        assert_eq!(taxonomy_keys("category"), vec!["categories", "category"]);
        assert_eq!(taxonomy_keys("tags"), vec!["tags", "tag"]);
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

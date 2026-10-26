//! `forno build`

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::assets;
use crate::config::BuildConfig;
use crate::diag::{self, Diagnostic};
use crate::postprocess;
use crate::say;
use crate::site::{Mode, Route, Site, Source, page_url};

#[derive(Default)]
pub struct BuildOptions {
    pub url: Option<String>,
    pub out: Option<PathBuf>,
    /// Skip Tailwind/esbuild (copy assets as-is). Useful for CI without Node.
    pub skip_tools: bool,
    /// Override `build.minify` (e.g. `--no-minify`, and `forno check`).
    pub minify: Option<bool>,
    /// Override `build.hash_assets`.
    pub hash_assets: Option<bool>,
}

/// One written HTML page.
#[derive(Debug, Clone, Serialize)]
pub struct PageInfo {
    pub url: String,
    pub file: String,
    pub source: String,
    pub kind: &'static str,
    pub title: String,
    pub description: String,
    #[serde(skip)]
    pub markdown: Option<String>,
    #[serde(skip)]
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub ok: bool,
    pub pages: usize,
    pub out_dir: PathBuf,
    pub duration_ms: u128,
    pub files: Vec<String>,
    pub warnings: Vec<Diagnostic>,
    #[serde(skip)]
    pub page_info: Vec<PageInfo>,
}

pub fn output_dir(site: &Site, cli: Option<&Path>) -> PathBuf {
    let dir = cli
        .map(Path::to_path_buf)
        .or_else(|| site.config.build.directory.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("_site"));
    if dir.is_absolute() {
        dir
    } else {
        site.root.join(dir)
    }
}

/// Refuse to wipe anything that looks like the project itself.
fn check_safe_to_clean(root: &Path, out: &Path) -> Result<()> {
    let out_abs = if out.exists() {
        out.canonicalize()?
    } else {
        out.to_path_buf()
    };
    if root.starts_with(&out_abs) {
        bail!(
            "refusing to use {} as the build directory: it contains the project",
            out.display()
        );
    }
    for protected in [
        "pages",
        "layouts",
        "includes",
        "content",
        "collections",
        "assets",
        "public",
    ] {
        if out_abs == root.join(protected) {
            bail!("refusing to use the `{protected}` folder as the build directory");
        }
    }
    if out_abs.join("pages").is_dir() && out_abs.join("layouts").is_dir() {
        bail!(
            "refusing to clean {}: it looks like a Forno project",
            out.display()
        );
    }
    Ok(())
}

fn kind(source: &Source) -> &'static str {
    match source {
        Source::Page(_) => "page",
        Source::Content(_) => "content",
        Source::Term { .. } => "term",
    }
}

/// Render one route (and its extra pagination pages); returns written pages.
fn build_route(
    site: &Site,
    r: &Route,
    out: &Path,
    assets_map: &HashMap<String, String>,
    minify: bool,
    images: bool,
) -> Result<Vec<PageInfo>> {
    let mut written = Vec::new();
    let mut page = 1;
    loop {
        let rendered = site.render(&r.source, &r.url, page)?;
        let (url, file) = if page == 1 {
            (r.url.clone(), r.out.clone())
        } else {
            let u = page_url(&r.url, page);
            let f = PathBuf::from(u.trim_start_matches('/')).join("index.html");
            (u, f)
        };
        let mut html = postprocess::rewrite_assets(&rendered.html, assets_map);
        if images {
            html = postprocess::optimize_images(&html, out, &site.base_url);
        }
        let text = postprocess::main_text(&html);
        if minify {
            html = postprocess::minify(&html);
        }
        let target = out.join(&file);
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&target, html)?;
        written.push(PageInfo {
            url,
            file: file.to_string_lossy().replace('\\', "/"),
            source: site.rel(r.source.path()),
            kind: kind(&r.source),
            title: rendered.title,
            description: rendered.description,
            markdown: if page == 1 { rendered.markdown } else { None },
            text,
        });
        if page >= rendered.total_pages {
            break;
        }
        page += 1;
    }
    Ok(written)
}

pub fn build(root: &Path, opts: BuildOptions) -> Result<Report> {
    let started = Instant::now();
    let site = Site::load(root, Mode::Build, opts.url)?;
    let cfg = &site.config.build;
    let out = output_dir(&site, opts.out.as_deref());
    check_safe_to_clean(&site.root, &out)?;

    if out.exists() {
        std::fs::remove_dir_all(&out).with_context(|| format!("cannot clean {}", out.display()))?;
    }
    std::fs::create_dir_all(&out)?;

    // 1. Plain files.
    assets::copy_dir(&site.root.join("public"), &out)?;
    assets::copy_dir(&site.root.join("assets"), &out.join("assets"))?;

    // 2. CSS and JS, in parallel.
    if !opts.skip_tools {
        let (css, js) = std::thread::scope(|s| {
            let css = s.spawn(|| assets::build_css(&site, &out));
            let js = s.spawn(|| assets::build_js(&site, &out));
            (
                css.join().expect("css thread"),
                js.join().expect("js thread"),
            )
        });
        js?;
        site.css_compiled.store(css?, Ordering::Relaxed);
    }

    // 3. Fingerprint CSS/JS for long-term caching.
    let assets_map = if opts
        .hash_assets
        .unwrap_or(BuildConfig::flag(cfg.hash_assets, true))
    {
        postprocess::fingerprint_assets(&out)?
    } else {
        HashMap::new()
    };

    // 4. Pages, in parallel. Collect every error instead of stopping at the first.
    let minify = opts.minify.unwrap_or(BuildConfig::flag(cfg.minify, true));
    let images = BuildConfig::flag(cfg.images, true);
    let routes = site.routes()?;
    let results: Vec<Result<Vec<PageInfo>>> = routes
        .par_iter()
        .map(|r| build_route(&site, r, &out, &assets_map, minify, images))
        .collect();
    let mut pages: Vec<PageInfo> = Vec::new();
    let mut errors = Vec::new();
    for r in results {
        match r {
            Ok(p) => pages.extend(p),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    if !errors.is_empty() {
        let e = BuildError {
            errors: errors.iter().map(|e| diag::parse(e, "error")).collect(),
        };
        return Err(e.into());
    }
    pages.sort_by(|a, b| a.url.cmp(&b.url));
    let mut files: Vec<String> = pages.iter().map(|p| p.file.clone()).collect();

    // 5. Feeds.
    for feed in crate::feeds::configs(&site) {
        let xml = crate::feeds::atom(&site, &feed)?;
        let rel = feed.url.trim_start_matches('/').to_string();
        let target = out.join(&rel);
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(target, xml)?;
        files.push(rel);
    }

    // 6. sitemap.xml (only meaningful with an absolute URL).
    if site.base_url != "relative"
        && BuildConfig::flag(cfg.sitemap, true)
        && !site.root.join("public/sitemap.xml").exists()
    {
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
        );
        for p in pages.iter().filter(|p| p.url != "/404.html") {
            xml.push_str(&format!(
                "  <url><loc>{}</loc></url>\n",
                xml_escape(&site.abs_url(&p.url))
            ));
        }
        xml.push_str("</urlset>\n");
        std::fs::write(out.join("sitemap.xml"), xml)?;
        files.push("sitemap.xml".into());
    }

    // 7. AI-friendly outputs: markdown mirrors, llms.txt, llms-full.txt.
    let mirrors = BuildConfig::flag(cfg.markdown_mirrors, true);
    if mirrors {
        for p in &pages {
            if let Some(md) = &p.markdown {
                let rel = mirror_path(&p.url);
                let body = format!("# {}\n\n{}\n", p.title, md);
                std::fs::write(out.join(&rel), body)?;
                files.push(rel);
            }
        }
    }
    if BuildConfig::flag(cfg.llms, true) && !site.root.join("public/llms.txt").exists() {
        let (llms, full) = llms_txt(&site, &pages, mirrors);
        std::fs::write(out.join("llms.txt"), llms)?;
        std::fs::write(out.join("llms-full.txt"), full)?;
        files.push("llms.txt".into());
        files.push("llms-full.txt".into());
    }

    // 8. Search.
    match &cfg.search {
        Some(serde_json::Value::Bool(true)) => {
            let index: Vec<_> = pages
                .iter()
                .filter(|p| p.url != "/404.html" && p.kind != "term" && !p.url.contains("/page/"))
                .map(|p| {
                    let text: String = p.text.chars().take(5000).collect();
                    json!({"url": p.url, "title": p.title, "description": p.description, "text": text})
                })
                .collect();
            std::fs::write(out.join("search.json"), serde_json::to_string(&index)?)?;
            files.push("search.json".into());
        }
        Some(serde_json::Value::String(s)) if s == "pagefind" => {
            run_pagefind(&site.root, &out)?;
        }
        _ => {}
    }

    let report = Report {
        ok: true,
        pages: pages.len(),
        duration_ms: started.elapsed().as_millis(),
        out_dir: out.clone(),
        files,
        warnings: diag::take_warnings(),
        page_info: pages,
    };
    let content = report
        .page_info
        .iter()
        .filter(|p| p.kind == "content")
        .count();
    say!(
        "🔥 Baked {} page(s) ({content} from markdown) into {} in {} ms",
        report.pages,
        site.rel(&out).trim_start_matches("./"),
        report.duration_ms
    );
    Ok(report)
}

/// All page errors of a failed build, for human or JSON output.
#[derive(Debug)]
pub struct BuildError {
    pub errors: Vec<Diagnostic>,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} page(s) failed to build:", self.errors.len())?;
        for e in &self.errors {
            let loc = match (&e.file, e.line) {
                (Some(file), Some(l)) => format!("{file}:{l}: "),
                (Some(file), None) => format!("{file}: "),
                _ => String::new(),
            };
            writeln!(f, "  - {loc}{}", e.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for BuildError {}

/// `/blog/post/` → `blog/post.md`, `/` → `index.md`.
pub fn mirror_path(url: &str) -> String {
    let t = url.trim_matches('/');
    if t.is_empty() {
        "index.md".into()
    } else {
        format!("{t}.md")
    }
}

fn llms_txt(site: &Site, pages: &[PageInfo], mirrors: bool) -> (String, String) {
    let name = site
        .site_str("name")
        .or_else(|| {
            pages
                .iter()
                .find(|p| p.url == "/")
                .map(|p| p.title.clone())
                .filter(|t| !t.is_empty())
        })
        .unwrap_or_else(|| "Website".into());
    let mut s = format!("# {name}\n\n");
    if let Some(d) = site.site_str("description") {
        s.push_str(&format!("> {d}\n\n"));
    }
    let listed: Vec<&PageInfo> = pages
        .iter()
        .filter(|p| p.url != "/404.html" && p.kind != "term" && !p.url.contains("/page/"))
        .collect();
    let line = |p: &PageInfo| {
        let link = if mirrors && p.markdown.is_some() {
            site.abs_url(&format!("/{}", mirror_path(&p.url)))
        } else {
            site.abs_url(&p.url)
        };
        let title = if p.title.is_empty() {
            p.url.clone()
        } else {
            p.title.clone()
        };
        if p.description.is_empty() {
            format!("- [{title}]({link})\n")
        } else {
            format!(
                "- [{title}]({link}): {}\n",
                p.description.replace('\n', " ")
            )
        }
    };
    s.push_str("## Pages\n\n");
    for p in listed.iter().filter(|p| p.kind == "page") {
        s.push_str(&line(p));
    }
    // One section per content folder.
    let mut sections: std::collections::BTreeMap<String, Vec<&PageInfo>> = Default::default();
    for p in listed.iter().filter(|p| p.kind == "content") {
        let folder = p
            .url
            .trim_matches('/')
            .split('/')
            .next()
            .unwrap_or("content");
        let folder = if p.url.trim_matches('/').contains('/') {
            folder
        } else {
            "Content"
        };
        sections.entry(folder.to_string()).or_default().push(p);
    }
    for (folder, items) in sections {
        let mut title = folder.replace(['-', '_'], " ");
        if let Some(f) = title.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        s.push_str(&format!("\n## {title}\n\n"));
        for p in items {
            s.push_str(&line(p));
        }
    }

    let mut full = format!("# {name}\n\n");
    for p in &listed {
        full.push_str(&format!(
            "---\n\n# {}\n\nURL: {}\n\n",
            p.title,
            site.abs_url(&p.url)
        ));
        match &p.markdown {
            Some(md) => full.push_str(md),
            None => full.push_str(&p.text),
        }
        full.push_str("\n\n");
    }
    (s, full)
}

fn run_pagefind(root: &Path, out: &Path) -> Result<()> {
    let (program, mut args): (PathBuf, Vec<String>) = if let Some(p) = assets::which("pagefind") {
        (p, vec![])
    } else if let Some(npx) = assets::which("npx") {
        (npx, vec!["--yes".into(), "pagefind".into()])
    } else {
        diag::warn(
            "search = \"pagefind\" but neither `pagefind` nor `npx` is available; skipping search index",
        );
        return Ok(());
    };
    args.extend(["--site".into(), out.display().to_string()]);
    let o = std::process::Command::new(program)
        .args(&args)
        .current_dir(root)
        .output()?;
    if !o.status.success() {
        bail!("pagefind failed:\n{}", String::from_utf8_lossy(&o.stderr));
    }
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

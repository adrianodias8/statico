//! `static build`

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::assets;
use crate::site::{Mode, Site, Source};

pub struct BuildOptions {
    pub url: Option<String>,
    pub out: Option<PathBuf>,
    /// Skip Tailwind/esbuild (copy assets as-is). Useful for CI without Node.
    pub skip_tools: bool,
}

pub struct Report {
    pub pages: usize,
    pub out_dir: PathBuf,
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
            "refusing to clean {}: it looks like a Static project",
            out.display()
        );
    }
    Ok(())
}

pub fn build(root: &Path, opts: BuildOptions) -> Result<Report> {
    let started = Instant::now();
    let site = Site::load(root, Mode::Build, opts.url)?;
    let out = output_dir(&site, opts.out.as_deref());
    check_safe_to_clean(&site.root, &out)?;

    if out.exists() {
        std::fs::remove_dir_all(&out).with_context(|| format!("cannot clean {}", out.display()))?;
    }
    std::fs::create_dir_all(&out)?;

    // 1. Static files.
    assets::copy_dir(&site.root.join("public"), &out)?;
    assets::copy_dir(&site.root.join("assets"), &out.join("assets"))?;

    // 2. CSS and JS, in parallel.
    if !opts.skip_tools {
        if site.tailwind == crate::site::Tailwind::V3 {
            assets::ensure_node_modules(&site.root)?;
        }
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

    // 3. Pages, in parallel. Collect every error instead of stopping at the first.
    let routes = site.routes()?;
    let errors: Vec<String> = routes
        .par_iter()
        .filter_map(|r| {
            let res = (|| -> Result<()> {
                let html = site.render(&r.source, &r.url)?;
                let target = out.join(&r.out);
                if let Some(p) = target.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::write(&target, html)?;
                Ok(())
            })();
            res.err().map(|e| format!("{e:#}"))
        })
        .collect();
    if !errors.is_empty() {
        bail!(
            "{} page(s) failed to build:\n  - {}",
            errors.len(),
            errors.join("\n  - ")
        );
    }

    // 4. sitemap.xml (only meaningful with an absolute URL).
    if site.base_url != "relative"
        && site.config.build.sitemap.unwrap_or(true)
        && !site.root.join("public/sitemap.xml").exists()
    {
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
        );
        let mut urls: Vec<&str> = routes
            .iter()
            .filter(|r| r.url != "/404.html")
            .map(|r| r.url.as_str())
            .collect();
        urls.sort();
        for u in urls {
            xml.push_str(&format!(
                "  <url><loc>{}{}</loc></url>\n",
                site.base_url,
                xml_escape(u)
            ));
        }
        xml.push_str("</urlset>\n");
        std::fs::write(out.join("sitemap.xml"), xml)?;
    }

    let pages = routes.len();
    let content = routes
        .iter()
        .filter(|r| matches!(r.source, Source::Content(_)))
        .count();
    println!(
        "Built {pages} page(s) ({content} from markdown) into {} in {:.0?} 🤘",
        site.rel(&out).trim_start_matches("./"),
        started.elapsed()
    );
    Ok(Report {
        pages,
        out_dir: out,
    })
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

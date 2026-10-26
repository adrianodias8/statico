//! `forno.json` (or the legacy `static.json`), and `.env`. All optional.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub build: BuildConfig,
    pub dev: DevConfig,
    /// Extra HTTP headers sent by `forno dev` / `forno serve`.
    pub headers: Map<String, Value>,
    /// Arbitrary site-wide data, available in templates as `{site.key}`.
    /// `site.name`, `site.description`, `site.image`, `site.author` and
    /// `site.language` also feed `{seo}`, feeds and `llms.txt`.
    pub site: Value,
    pub markdown: MarkdownConfig,
    /// `false` disables feeds; a list configures them. Default: one Atom feed
    /// per top-level content folder whose posts have a `date`.
    pub feeds: Option<Value>,
    /// Front matter schemas per content folder, e.g.
    /// `{"posts": {"title": "string", "date": "date", "tags": "list?"}}`.
    pub schemas: Map<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    /// Absolute base URL used by `{ url('/path') }`, feeds and the sitemap.
    pub url: Option<String>,
    /// Output directory (default `_site`).
    pub directory: Option<String>,
    /// Generate `sitemap.xml` when `url` is absolute (default true).
    pub sitemap: Option<bool>,
    /// Minify HTML (default true).
    pub minify: Option<bool>,
    /// Fingerprint CSS/JS file names for long-term caching (default true).
    pub hash_assets: Option<bool>,
    /// Write `llms.txt` (default true).
    pub llms: Option<bool>,
    /// Write a `.md` copy of every markdown page for LLMs (default true).
    pub markdown_mirrors: Option<bool>,
    /// `true` writes `search.json` for client-side search; `"pagefind"` runs
    /// Pagefind over the output (default off).
    pub search: Option<Value>,
    /// Add width/height and lazy loading to local images (default true).
    pub images: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DevConfig {
    pub url: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MarkdownConfig {
    /// Code highlighting theme name, or `false` to disable.
    pub highlight: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedConfig {
    /// Content folder, e.g. `posts`.
    pub content: String,
    /// Output path (default `<folder>/feed.xml`).
    pub path: Option<String>,
    pub title: Option<String>,
    /// Number of entries (default 20).
    pub limit: Option<usize>,
}

impl BuildConfig {
    pub fn flag(v: Option<bool>, default: bool) -> bool {
        v.unwrap_or(default)
    }
}

impl MarkdownConfig {
    /// `None` = highlighting off.
    pub fn theme(&self) -> Option<String> {
        match &self.highlight {
            None | Some(Value::Bool(true)) | Some(Value::Null) => {
                Some(crate::markdown::DEFAULT_THEME.to_string())
            }
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let Some(p) = Self::path(root) else {
            return Ok(Self::default());
        };
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&p).with_context(|| format!("cannot read {name}"))?;
        serde_json::from_str(&text).with_context(|| format!("{name} is not valid"))
    }

    /// `forno.json`, falling back to `static.json` for projects made for Static.
    pub fn path(root: &Path) -> Option<std::path::PathBuf> {
        ["forno.json", "static.json"]
            .iter()
            .map(|n| root.join(n))
            .find(|p| p.is_file())
    }
}

/// Variables from `.env`, exposed as `{env.KEY}`. The process environment is
/// deliberately *not* exposed, so secrets in your shell never leak into HTML.
pub fn load_env(root: &Path) -> Map<String, Value> {
    std::fs::read_to_string(root.join(".env"))
        .map(|s| parse_env(&s))
        .unwrap_or_default()
}

/// Lenient `.env` parser: `KEY=value`, optional `export `, quotes, `#` comments.
pub fn parse_env(src: &str) -> Map<String, Value> {
    let mut m = Map::new();
    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        let v = if v.len() >= 2
            && ((v.starts_with('"') && v.ends_with('"'))
                || (v.starts_with('\'') && v.ends_with('\'')))
        {
            v[1..v.len() - 1].replace("\\n", "\n")
        } else {
            // Strip trailing ` # comment` on unquoted values.
            v.split(" #").next().unwrap_or(v).trim().to_string()
        };
        m.insert(k.trim().to_string(), Value::String(v));
    }
    m
}

#[cfg(test)]
mod tests {
    #[test]
    fn static_json_fallback_and_precedence() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("static.json"),
            r#"{"build": {"url": "https://old"}}"#,
        )
        .unwrap();
        assert_eq!(
            super::Config::load(d.path()).unwrap().build.url.as_deref(),
            Some("https://old")
        );
        std::fs::write(
            d.path().join("forno.json"),
            r#"{"build": {"url": "https://new"}}"#,
        )
        .unwrap();
        assert_eq!(
            super::Config::load(d.path()).unwrap().build.url.as_deref(),
            Some("https://new")
        );
    }

    #[test]
    fn env() {
        let m = super::parse_env("# c\nA=hello world\nexport B=\"q x\" \nC='s' \nD=v # note\nbad");
        assert_eq!(m["A"], "hello world");
        assert_eq!(m["B"], "q x");
        assert_eq!(m["C"], "s");
        assert_eq!(m["D"], "v");
        assert_eq!(m.len(), 4);
    }
}

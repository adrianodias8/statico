//! `static.json` (optional) and `.env` (optional).

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub build: BuildConfig,
    pub dev: DevConfig,
    /// Extra HTTP headers sent by `static dev` / `static serve`.
    pub headers: Map<String, Value>,
    /// Arbitrary site-wide data, available in templates as `{site.key}`.
    pub site: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    /// Absolute base URL used by `{ url('/path') }` and the sitemap.
    pub url: Option<String>,
    /// Output directory (default `_site`).
    pub directory: Option<String>,
    /// Generate `sitemap.xml` when `url` is absolute (default true).
    pub sitemap: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DevConfig {
    pub url: Option<String>,
    pub port: Option<u16>,
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let p = root.join("static.json");
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&p).context("cannot read static.json")?;
        serde_json::from_str(&text).context("static.json is not valid JSON")
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
    fn env() {
        let m = super::parse_env("# c\nA=hello world\nexport B=\"q x\" \nC='s' \nD=v # note\nbad");
        assert_eq!(m["A"], "hello world");
        assert_eq!(m["B"], "q x");
        assert_eq!(m["C"], "s");
        assert_eq!(m["D"], "v");
        assert_eq!(m.len(), 4);
    }
}

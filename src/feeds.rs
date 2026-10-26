//! Atom feeds for content folders.
//!
//! Zero-config: every top-level `content/<folder>` whose posts have a `date`
//! gets `/<folder>/feed.xml`. Configure or disable with `feeds` in forno.json.

use anyhow::Result;
use serde_json::Value;

use crate::config::FeedConfig;
use crate::site::Site;
use crate::template::filters::{escape_html, parse_date};
use crate::template::{DataSource, value_to_string};

#[derive(Debug, Clone)]
pub struct Feed {
    pub content: String,
    pub title: String,
    /// Path, e.g. `/posts/feed.xml`.
    pub url: String,
    pub limit: usize,
}

pub fn configs(site: &Site) -> Vec<Feed> {
    let site_name = site.site_str("name").unwrap_or_else(|| "Feed".into());
    let make = |c: FeedConfig| Feed {
        title: c.title.unwrap_or_else(|| {
            let folder = c.content.trim_matches('/');
            let mut t = folder.replace(['-', '_'], " ");
            if let Some(f) = t.get_mut(0..1) {
                f.make_ascii_uppercase();
            }
            format!("{site_name} · {t}")
        }),
        url: format!(
            "/{}",
            c.path
                .unwrap_or_else(|| format!("{}/feed.xml", c.content.trim_matches('/')))
                .trim_start_matches('/')
        ),
        limit: c.limit.unwrap_or(20),
        content: c.content,
    };
    match &site.config.feeds {
        Some(Value::Bool(false)) => vec![],
        Some(v @ Value::Array(_)) => serde_json::from_value::<Vec<FeedConfig>>(v.clone())
            .map(|v| v.into_iter().map(make).collect())
            .unwrap_or_else(|e| {
                crate::diag::warn(format!("forno.json: invalid `feeds`: {e}"));
                vec![]
            }),
        // Auto feeds need absolute links, so they are only made when the
        // site has a URL (`forno build https://…` or `build.url`).
        _ if site.base_url == "relative" => vec![],
        _ => {
            // Auto: top-level content folders with dated posts.
            let Ok(entries) = std::fs::read_dir(site.root.join("content")) else {
                return vec![];
            };
            let mut dirs: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|d| !d.starts_with('.') && !d.starts_with('_'))
                .collect();
            dirs.sort();
            dirs.into_iter()
                .filter(|d| {
                    site.content_collection(d).is_ok_and(|v| {
                        v.as_array()
                            .is_some_and(|a| a.iter().any(|i| i.get("date").is_some()))
                    })
                })
                .map(|d| {
                    make(FeedConfig {
                        content: d,
                        path: None,
                        title: None,
                        limit: None,
                    })
                })
                .collect()
        }
    }
}

fn rfc3339(s: &str) -> Option<String> {
    parse_date(s).map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// Render an Atom 1.0 document.
pub fn atom(site: &Site, feed: &Feed) -> Result<String> {
    let mut items = site
        .content_collection(&feed.content)?
        .as_array()
        .cloned()
        .unwrap_or_default();
    items.retain(|i| i.get("date").is_some());
    items.sort_by(|a, b| {
        let da = a.get("date").map(value_to_string).and_then(|d| rfc3339(&d));
        let db = b.get("date").map(value_to_string).and_then(|d| rfc3339(&d));
        db.cmp(&da)
    });
    items.truncate(feed.limit);

    if site.base_url == "relative" {
        crate::diag::warn(format!(
            "{}: feed links are relative; build with a URL (`forno build https://example.com`) for valid feeds",
            feed.url
        ));
    }
    let updated = items
        .first()
        .and_then(|i| i.get("date"))
        .map(value_to_string)
        .and_then(|d| rfc3339(&d))
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let self_url = site.abs_url(&feed.url);
    let home = site.abs_url("/");
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">\n",
    );
    x.push_str(&format!("  <title>{}</title>\n", escape_html(&feed.title)));
    if let Some(d) = site.site_str("description") {
        x.push_str(&format!("  <subtitle>{}</subtitle>\n", escape_html(&d)));
    }
    x.push_str(&format!(
        "  <link href=\"{}\" rel=\"self\"/>\n",
        escape_html(&self_url)
    ));
    x.push_str(&format!("  <link href=\"{}\"/>\n", escape_html(&home)));
    x.push_str(&format!("  <id>{}</id>\n", escape_html(&self_url)));
    x.push_str(&format!("  <updated>{updated}</updated>\n"));
    if let Some(a) = site.site_str("author") {
        x.push_str(&format!(
            "  <author><name>{}</name></author>\n",
            escape_html(&a)
        ));
    }
    x.push_str("  <generator>Forno</generator>\n");
    for i in items {
        let s = |k: &str| i.get(k).map(value_to_string).unwrap_or_default();
        let link = site.abs_url(&s("url"));
        let date = rfc3339(&s("date")).unwrap_or_else(|| updated.clone());
        x.push_str("  <entry>\n");
        x.push_str(&format!(
            "    <title>{}</title>\n",
            escape_html(&s("title"))
        ));
        x.push_str(&format!("    <link href=\"{}\"/>\n", escape_html(&link)));
        x.push_str(&format!("    <id>{}</id>\n", escape_html(&link)));
        x.push_str(&format!(
            "    <published>{date}</published>\n    <updated>{date}</updated>\n"
        ));
        let author = ["author", "authorName"]
            .iter()
            .map(|k| s(k))
            .find(|a| !a.is_empty());
        if let Some(a) = author {
            x.push_str(&format!(
                "    <author><name>{}</name></author>\n",
                escape_html(&a)
            ));
        }
        let summary = ["description", "excerpt", "summary"]
            .iter()
            .map(|k| s(k))
            .find(|a| !a.is_empty());
        if let Some(sm) = summary {
            x.push_str(&format!("    <summary>{}</summary>\n", escape_html(&sm)));
        }
        x.push_str(&format!(
            "    <content type=\"html\">{}</content>\n",
            escape_html(&s("html"))
        ));
        x.push_str("  </entry>\n");
    }
    x.push_str("</feed>\n");
    Ok(x)
}

//! `{seo}`: description, canonical, Open Graph, Twitter, feeds and JSON-LD,
//! derived from front matter and `site` in forno.json.

use serde_json::{Value, json};

use crate::site::{Site, script_json};
use crate::template::filters::{escape_html, parse_date};
use crate::template::value_to_string;

pub struct PageMeta<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub url: &'a str,
    pub front_matter: &'a Value,
    pub is_article: bool,
}

fn meta(out: &mut String, attr: &str, key: &str, value: &str) {
    if !value.is_empty() {
        out.push_str(&format!(
            "<meta {attr}=\"{key}\" content=\"{}\">\n",
            escape_html(value)
        ));
    }
}

pub fn tags(site: &Site, page: &PageMeta) -> String {
    let fm = page.front_matter;
    let fm_str = |k: &str| fm.get(k).map(value_to_string).filter(|s| !s.is_empty());
    let site_name = site.site_str("name").unwrap_or_default();
    let image = [
        "image",
        "ogImage",
        "ogImageUrl",
        "og_image",
        "coverImage",
        "cover",
    ]
    .iter()
    .find_map(|k| fm_str(k))
    .or_else(|| site.site_str("image"))
    .map(|i| site.abs_url(&i))
    .unwrap_or_default();
    let url = site.abs_url(page.url);
    let absolute = site.base_url != "relative";

    let mut out = String::new();
    meta(&mut out, "name", "description", page.description);
    if absolute {
        out.push_str(&format!(
            "<link rel=\"canonical\" href=\"{}\">\n",
            escape_html(&url)
        ));
    }
    meta(&mut out, "property", "og:title", page.title);
    meta(&mut out, "property", "og:description", page.description);
    meta(
        &mut out,
        "property",
        "og:type",
        if page.is_article {
            "article"
        } else {
            "website"
        },
    );
    if absolute {
        meta(&mut out, "property", "og:url", &url);
    }
    meta(&mut out, "property", "og:site_name", &site_name);
    meta(&mut out, "property", "og:image", &image);
    if let Some(lang) = site.site_str("language") {
        meta(&mut out, "property", "og:locale", &lang.replace('-', "_"));
    }
    meta(
        &mut out,
        "name",
        "twitter:card",
        if image.is_empty() {
            "summary"
        } else {
            "summary_large_image"
        },
    );
    meta(&mut out, "name", "twitter:title", page.title);
    meta(&mut out, "name", "twitter:description", page.description);
    meta(&mut out, "name", "twitter:image", &image);
    if let Some(handle) = site.site_str("twitter") {
        meta(&mut out, "name", "twitter:site", &handle);
    }

    let date = fm_str("date").and_then(|d| parse_date(&d));
    if page.is_article
        && let Some(d) = date
    {
        meta(
            &mut out,
            "property",
            "article:published_time",
            &d.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );
    }

    for feed in crate::feeds::configs(site) {
        out.push_str(&format!(
            "<link rel=\"alternate\" type=\"application/atom+xml\" title=\"{}\" href=\"{}\">\n",
            escape_html(&feed.title),
            escape_html(&site.abs_url(&feed.url))
        ));
    }

    // JSON-LD structured data.
    let author = fm_str("author")
        .or_else(|| fm_str("authorName"))
        .or_else(|| site.site_str("author"));
    let ld = if page.is_article {
        let mut v = json!({
            "@context": "https://schema.org",
            "@type": "BlogPosting",
            "headline": page.title,
            "description": page.description,
        });
        if absolute {
            v["url"] = json!(url);
        }
        if !image.is_empty() {
            v["image"] = json!(image);
        }
        if let Some(d) = date {
            v["datePublished"] = json!(d.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        }
        if let Some(a) = author {
            v["author"] = json!({"@type": "Person", "name": a});
        }
        Some(v)
    } else if page.url == "/" && !site_name.is_empty() {
        let mut v =
            json!({"@context": "https://schema.org", "@type": "WebSite", "name": site_name});
        if absolute {
            v["url"] = json!(format!("{}/", site.base_url));
        }
        Some(v)
    } else {
        None
    };
    if let Some(ld) = ld {
        out.push_str(&format!(
            "<script type=\"application/ld+json\">{}</script>\n",
            script_json(&ld)
        ));
    }
    out
}

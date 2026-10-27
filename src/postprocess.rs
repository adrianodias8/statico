//! Build-time HTML post-processing: image dimensions + lazy loading,
//! fingerprinted asset URLs, minification, and plain-text extraction.

use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::template::filters::strip_html;
use crate::template::tags::{find_tags, scan_tag};

/// Map a URL path (`/assets/a.png`, possibly with the absolute base) to a
/// file in the output directory.
fn local_file(out_dir: &Path, base_url: &str, src: &str) -> Option<PathBuf> {
    let src = if base_url != "relative" {
        src.strip_prefix(base_url).unwrap_or(src)
    } else {
        src
    };
    if !src.starts_with('/') || src.starts_with("//") {
        return None;
    }
    let path = src.split(['?', '#']).next()?;
    let p = crate::site::percent_decode(path);
    if p.split('/').any(|s| s == "..") {
        return None;
    }
    Some(out_dir.join(p.trim_start_matches('/')))
}

/// Add `width`/`height` (prevents layout shift) and `loading="lazy"` +
/// `decoding="async"` to local `<img>` tags. The first image is left eager
/// because it is usually the largest contentful paint.
pub fn optimize_images(html: &str, out_dir: &Path, base_url: &str) -> String {
    if !html.contains("<img") {
        return html.to_string();
    }
    let tags = find_tags(html, &["img"]);
    let mut out = String::with_capacity(html.len() + tags.len() * 40);
    let mut pos = 0;
    for (i, t) in tags.iter().enumerate() {
        if t.closing {
            continue;
        }
        out.push_str(&html[pos..t.start]);
        pos = t.end;
        let mut extra = String::new();
        let has = |k: &str| t.attr(k).is_some();
        if !has("width")
            && !has("height")
            && let Some(file) = t.attr("src").and_then(|s| local_file(out_dir, base_url, s))
            && let Ok(dim) = imagesize::size(&file)
        {
            extra.push_str(&format!(
                " width=\"{}\" height=\"{}\"",
                dim.width, dim.height
            ));
        }
        if i > 0 && !has("loading") {
            extra.push_str(" loading=\"lazy\"");
        }
        if !has("decoding") {
            extra.push_str(" decoding=\"async\"");
        }
        let original = &html[t.start..t.end];
        if extra.is_empty() {
            out.push_str(original);
        } else {
            // Insert before the closing `>` or `/>`.
            let cut = if original.ends_with("/>") {
                original.len() - 2
            } else {
                original.len() - 1
            };
            let head = original[..cut].trim_end();
            out.push_str(head);
            out.push_str(&extra);
            if original.ends_with("/>") {
                out.push(' ');
            }
            out.push_str(&original[cut..]);
        }
    }
    out.push_str(&html[pos..]);
    out
}

/// The path a site is served under, from its base URL:
/// `https://user.github.io/project` gives `/project`. Empty for a site at the
/// root of its domain, and for relative builds.
pub fn base_path(base_url: &str) -> &str {
    if base_url == "relative" {
        return "";
    }
    let rest = base_url.split_once("://").map_or(base_url, |(_, r)| r);
    rest.find('/')
        .map_or("", |i| rest[i..].trim_end_matches('/'))
}

/// `value` moved under `prefix` if it is root-relative (`/docs/`), and left
/// alone if it is absolute, protocol-relative (`//cdn`) or already inside.
fn under(prefix: &str, value: &str) -> Option<String> {
    let inside = value == prefix || value.starts_with(&format!("{prefix}/"));
    (value.starts_with('/') && !value.starts_with("//") && !inside)
        .then(|| format!("{prefix}{value}"))
}

static ROOT_ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(\s(?:href|src|action|poster)\s*=\s*)(["'])(/[^"']*)"#).unwrap()
});
static SRCSET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(\ssrcset\s*=\s*)(["'])([^"']*)"#).unwrap());
static CSS_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"url\(\s*(["']?)(/[^)"']*)"#).unwrap());

/// For a site served under a sub-path, such as a GitHub Pages project site at
/// `https://user.github.io/project`, point root-relative URLs in attributes
/// inside it: `href="/docs/"` becomes `href="/project/docs/"`. Without this
/// the pages load with no stylesheet and every internal link 404s.
pub fn prefix_root_paths(html: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return html.to_string();
    }
    let html = ROOT_ATTR.replace_all(html, |c: &regex::Captures| match under(prefix, &c[3]) {
        Some(v) => format!("{}{}{v}", &c[1], &c[2]),
        None => c[0].to_string(),
    });
    SRCSET
        .replace_all(&html, |c: &regex::Captures| {
            let list: Vec<String> = c[3]
                .split(',')
                .map(|part| {
                    let part = part.trim();
                    let (url, rest) = part.split_once(' ').unwrap_or((part, ""));
                    let url = under(prefix, url).unwrap_or_else(|| url.to_string());
                    if rest.is_empty() {
                        url
                    } else {
                        format!("{url} {rest}")
                    }
                })
                .collect();
            format!("{}{}{}", &c[1], &c[2], list.join(", "))
        })
        .into_owned()
}

/// The same for `url(/…)` in a stylesheet, e.g. self-hosted fonts.
pub fn prefix_css_paths(css: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return css.to_string();
    }
    CSS_URL
        .replace_all(css, |c: &regex::Captures| match under(prefix, &c[2]) {
            Some(v) => format!("url({}{v}", &c[1]),
            None => c[0].to_string(),
        })
        .into_owned()
}

/// Apply [`prefix_css_paths`] to every stylesheet under `dir`.
pub fn prefix_css_files(dir: &Path, prefix: &str) -> anyhow::Result<()> {
    if prefix.is_empty() || !dir.is_dir() {
        return Ok(());
    }
    for e in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "css") {
            let css = std::fs::read_to_string(e.path())?;
            let out = prefix_css_paths(&css, prefix);
            if out != css {
                std::fs::write(e.path(), out)?;
            }
        }
    }
    Ok(())
}

/// Replace `/assets/css/main.css` with its fingerprinted name everywhere,
/// only when followed by a URL-ending character (so `main.css.map` is safe).
pub fn rewrite_assets(html: &str, map: &HashMap<String, String>) -> String {
    let mut s = html.to_string();
    for (from, to) in map {
        if !s.contains(from.as_str()) {
            continue;
        }
        let mut out = String::with_capacity(s.len());
        let mut rest = s.as_str();
        while let Some(i) = rest.find(from.as_str()) {
            let after = rest[i + from.len()..].chars().next();
            out.push_str(&rest[..i]);
            if matches!(
                after,
                None | Some('"' | '\'' | '?' | '#' | ')' | ' ' | '>' | '\n')
            ) {
                out.push_str(to);
            } else {
                out.push_str(from);
            }
            rest = &rest[i + from.len()..];
        }
        out.push_str(rest);
        s = out;
    }
    s
}

/// Conservative HTML minifier: drops comments and indentation, collapses
/// whitespace runs; leaves `<pre>`, `<textarea>`, `<script>` and `<style>`
/// contents untouched. It never changes how a page renders.
pub fn minify(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while i < html.len() {
        // Raw-text elements are copied verbatim.
        if bytes[i] == b'<' {
            let mut raw = None;
            for tag in ["pre", "textarea", "script", "style"] {
                if lower[i + 1..].starts_with(tag)
                    && lower
                        .as_bytes()
                        .get(i + 1 + tag.len())
                        .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>')
                {
                    raw = Some(tag);
                    break;
                }
            }
            if let Some(tag) = raw {
                let end = lower[i..]
                    .find(&format!("</{tag}>"))
                    .map(|e| i + e + tag.len() + 3)
                    .unwrap_or(html.len());
                out.push_str(&html[i..end]);
                i = end;
                continue;
            }
            // Comments (keep conditional comments and SSI).
            if html[i..].starts_with("<!--")
                && !html[i..].starts_with("<!--[if")
                && !html[i..].starts_with("<!--#")
            {
                match html[i..].find("-->") {
                    Some(e) => {
                        i += e + 3;
                        continue;
                    }
                    None => {
                        out.push_str(&html[i..]);
                        break;
                    }
                }
            }
        }
        let c = html[i..].chars().next().unwrap();
        if c.is_whitespace() {
            let mut j = i;
            let mut newline = false;
            while j < html.len() {
                let d = html[j..].chars().next().unwrap();
                if !d.is_whitespace() {
                    break;
                }
                newline |= d == '\n';
                j += d.len_utf8();
            }
            match out.chars().last() {
                Some('\n') => {}
                Some(' ') if newline => {
                    out.pop();
                    out.push('\n');
                }
                Some(' ') => {}
                _ => out.push(if newline { '\n' } else { ' ' }),
            }
            i = j;
            continue;
        }
        out.push(c);
        i += c.len_utf8();
    }
    out.trim().to_string() + "\n"
}

/// Main text of a page for search and llms-full.txt: `<main>` (or `<article>`,
/// or `<body>`) without scripts, styles, nav, header and footer.
pub fn main_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let pick = |tag: &str| -> Option<(usize, usize)> {
        let start = lower.find(&format!("<{tag}"))?;
        let end = lower.rfind(&format!("</{tag}>"))?;
        (end > start).then_some((start, end))
    };
    let (a, b) = pick("main")
        .or_else(|| pick("article"))
        .or_else(|| pick("body"))
        .unwrap_or((0, html.len()));
    let mut region = html[a..b].to_string();
    for tag in [
        "script", "style", "nav", "header", "footer", "svg", "noscript",
    ] {
        region = remove_elements(&region, tag);
    }
    strip_html(&region)
}

fn remove_elements(html: &str, tag: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(off) = lower[pos..].find(&format!("<{tag}")) {
        let start = pos + off;
        // must be a real tag boundary
        if scan_tag(html, start, &[tag]).is_none() && scan_tag(&lower, start, &[tag]).is_none() {
            out.push_str(&html[pos..start + 1]);
            pos = start + 1;
            continue;
        }
        out.push_str(&html[pos..start]);
        match lower[start..].find(&format!("</{tag}>")) {
            Some(e) => pos = start + e + tag.len() + 3,
            None => {
                pos = html.len();
                break;
            }
        }
    }
    out.push_str(&html[pos.min(html.len())..]);
    out
}

/// Stable 64-bit FNV-1a, for asset fingerprints.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Rename CSS/JS under `<out>/assets` to `name.<hash>.ext`; returns the
/// URL rewrite map (`/assets/css/main.css` → `/assets/css/main.1a2b3c4d.css`).
pub fn fingerprint_assets(out_dir: &Path) -> anyhow::Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    let assets = out_dir.join("assets");
    if !assets.is_dir() {
        return Ok(map);
    }
    for e in walkdir::WalkDir::new(&assets) {
        let e = e?;
        let p = e.path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or_default();
        if !e.file_type().is_file() || !matches!(ext, "css" | "js") {
            continue;
        }
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(p)?;
        let hash = format!("{:016x}", fnv1a(&bytes));
        let new_name = format!("{stem}.{}.{ext}", &hash[..8]);
        let new_path = p.with_file_name(&new_name);
        std::fs::rename(p, &new_path)?;
        let rel = |q: &Path| {
            format!(
                "/{}",
                q.strip_prefix(out_dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            )
        };
        map.insert(rel(p), rel(&new_path));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    #[test]
    fn sub_path_sites_keep_their_links() {
        assert_eq!(
            super::base_path("https://user.github.io/project/"),
            "/project"
        );
        assert_eq!(super::base_path("https://example.com"), "");
        assert_eq!(super::base_path("relative"), "");
        let html = r#"<a href="/docs/">d</a><a href="/project/x/">x</a><a href="//cdn.example/a.js">c</a><a href="https://e.com/">e</a><img src="/a.png" srcset="/a.png 1x, /b.png 2x"><form action="/search/">"#;
        assert_eq!(
            super::prefix_root_paths(html, "/project"),
            r#"<a href="/project/docs/">d</a><a href="/project/x/">x</a><a href="//cdn.example/a.js">c</a><a href="https://e.com/">e</a><img src="/project/a.png" srcset="/project/a.png 1x, /project/b.png 2x"><form action="/project/search/">"#
        );
        assert_eq!(super::prefix_root_paths(html, ""), html);
        assert_eq!(
            super::prefix_css_paths(
                r#"src: url("/f.woff2") format("woff2"); background: url(/i.png); x: url(data:image/png;base64,AA)"#,
                "/project"
            ),
            r#"src: url("/project/f.woff2") format("woff2"); background: url(/project/i.png); x: url(data:image/png;base64,AA)"#
        );
    }

    use super::*;

    #[test]
    fn images_get_dimensions_and_lazy() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("img")).unwrap();
        // 1x1 PNG
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52,
            0, 0, 0, 3, 0, 0, 0, 2, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89,
        ];
        std::fs::write(d.path().join("img/a.png"), png).unwrap();
        let html = r#"<img src="/img/a.png" alt="hero"><p>x</p><img src="/img/a.png" alt="b" /><img src="https://e.com/x.png">"#;
        let out = optimize_images(html, d.path(), "relative");
        assert!(
            out.starts_with(
                r#"<img src="/img/a.png" alt="hero" width="3" height="2" decoding="async">"#
            ),
            "{out}"
        );
        assert!(
            out.contains(r#"alt="b" width="3" height="2" loading="lazy" decoding="async" />"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<img src="https://e.com/x.png" loading="lazy" decoding="async">"#),
            "{out}"
        );
    }

    #[test]
    fn asset_rewrite_respects_boundaries() {
        let mut m = HashMap::new();
        m.insert(
            "/assets/css/main.css".to_string(),
            "/assets/css/main.abc.css".to_string(),
        );
        let out = rewrite_assets(
            r#"<link href="/assets/css/main.css"><a href="/assets/css/main.css.map">"#,
            &m,
        );
        assert_eq!(
            out,
            r#"<link href="/assets/css/main.abc.css"><a href="/assets/css/main.css.map">"#
        );
    }

    #[test]
    fn text_extraction() {
        let t = main_text(
            "<html><body><nav>menu</nav><main><h1>Hi</h1><script>x()</script><p>there &amp; you</p></main><footer>f</footer></body></html>",
        );
        assert_eq!(t, "Hi there & you");
    }

    #[test]
    fn minifies() {
        let m = minify(
            "<html>\n  <head>\n  <title> x </title>\n</head>\n<body>\n  <!-- note -->\n  <p>a   b</p>\n  <pre>  keep\n    this</pre>\n</body>\n</html>",
        );
        assert_eq!(
            m,
            "<html>\n<head>\n<title> x </title>\n</head>\n<body>\n<p>a b</p>\n<pre>  keep\n    this</pre>\n</body>\n</html>\n"
        );
    }
}

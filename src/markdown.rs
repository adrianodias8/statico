//! Front matter, Markdown → HTML, heading anchors and `[toc]`.

use anyhow::{Context, Result};
use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub struct Document {
    pub front_matter: Map<String, Value>,
    pub body: String,
}

/// Split `---\nyaml\n---\nbody`. Invalid YAML is an error (the original
/// silently produced an empty object).
pub fn parse_front_matter(src: &str, origin: &str) -> Result<Document> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let normalized;
    let src = if src.contains("\r\n") {
        normalized = src.replace("\r\n", "\n");
        normalized.as_str()
    } else {
        src
    };
    if let Some(rest) = src
        .strip_prefix("---\n")
        .or_else(|| src.strip_prefix("---\r\n"))
    {
        // Find the closing fence on its own line.
        let mut offset = 0;
        for line in rest.split_inclusive('\n') {
            if line.trim_end() == "---" {
                let yaml = &rest[..offset];
                let body = &rest[offset + line.len()..];
                let fm: Value = if yaml.trim().is_empty() {
                    Value::Object(Map::new())
                } else {
                    serde_yaml::from_str(yaml)
                        .with_context(|| format!("{origin}: invalid front matter YAML"))?
                };
                let front_matter = match fm {
                    Value::Object(m) => m,
                    Value::Null => Map::new(),
                    _ => anyhow::bail!("{origin}: front matter must be a key/value mapping"),
                };
                return Ok(Document {
                    front_matter,
                    body: body.trim().to_string(),
                });
            }
            offset += line.len();
        }
    }
    Ok(Document {
        front_matter: Map::new(),
        body: src.trim().to_string(),
    })
}

/// GitHub-compatible heading slug.
pub fn slugify(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            c if c.is_whitespace() => Some('-'),
            _ => None,
        })
        .collect()
}

#[derive(Clone)]
pub struct Rendered {
    pub html: String,
    /// `[{content, slug, lvl}]`, compatible with markdown-toc's JSON.
    pub toc: Value,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
        | Options::ENABLE_HEADING_ATTRIBUTES
}

fn level_num(l: HeadingLevel) -> usize {
    l as usize
}

/// Render markdown; headings get GitHub-style `id`s and a `[toc]` marker
/// (on its own line) becomes a nested list of links.
/// Render with the default code theme.
pub fn render(markdown: &str) -> Rendered {
    render_with(markdown, Some(DEFAULT_THEME))
}

pub const DEFAULT_THEME: &str = "base16-ocean.dark";

static SYNTAXES: std::sync::LazyLock<syntect::parsing::SyntaxSet> =
    std::sync::LazyLock::new(syntect::parsing::SyntaxSet::load_defaults_newlines);
static THEMES: std::sync::LazyLock<syntect::highlighting::ThemeSet> =
    std::sync::LazyLock::new(syntect::highlighting::ThemeSet::load_defaults);

/// Names of the built-in code highlighting themes.
pub fn theme_names() -> Vec<String> {
    THEMES.themes.keys().cloned().collect()
}

fn highlight(code: &str, lang: &str, theme: &str) -> Option<String> {
    let lang = lang.split([' ', ',', '{']).next().unwrap_or_default();
    let syntax = SYNTAXES.find_syntax_by_token(lang).or_else(|| match lang {
        "html" | "vue" | "svelte" => SYNTAXES.find_syntax_by_extension("html"),
        "ts" | "typescript" | "tsx" | "jsx" => SYNTAXES.find_syntax_by_extension("js"),
        "sh" | "shell" | "zsh" | "console" => SYNTAXES.find_syntax_by_extension("sh"),
        "toml" => SYNTAXES.find_syntax_by_extension("yaml"),
        _ => None,
    })?;
    let theme = THEMES.themes.get(theme)?;
    let html = syntect::html::highlighted_html_for_string(code, &SYNTAXES, syntax, theme).ok()?;
    // Keep the language class so CSS/JS can still target it.
    Some(html.replacen("<pre ", &format!("<pre data-lang=\"{}\" ", esc(lang)), 1))
}

/// Render markdown; `theme` = `None` disables syntax highlighting.
pub fn render_with(markdown: &str, theme: Option<&str>) -> Rendered {
    let mut events: Vec<Event> = Parser::new_ext(markdown, options()).collect();

    // Pass 1: compute heading slugs.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut toc = Vec::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, id, .. }) = &events[i] {
            let level = *level;
            let explicit = id.clone();
            let mut text = String::new();
            let mut j = i + 1;
            while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                if let Event::Text(t) | Event::Code(t) = &events[j] {
                    text.push_str(t);
                }
                j += 1;
            }
            let slug = match explicit {
                Some(s) => s.to_string(),
                None => {
                    let base = slugify(&text);
                    let n = seen.entry(base.clone()).or_insert(0);
                    let s = if *n == 0 {
                        base.clone()
                    } else {
                        format!("{base}-{n}")
                    };
                    *n += 1;
                    s
                }
            };
            if let Event::Start(Tag::Heading { id, .. }) = &mut events[i] {
                *id = Some(CowStr::from(slug.clone()));
            }
            toc.push(json!({"content": text, "slug": slug, "lvl": level_num(level)}));
            i = j;
        }
        i += 1;
    }

    // Pass 2: fenced code → highlighted HTML; ```mermaid → <pre class="mermaid">.
    let mut out_events: Vec<Event> = Vec::with_capacity(events.len());
    let mut iter = events.into_iter();
    while let Some(ev) = iter.next() {
        if let Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(lang))) = &ev {
            let lang = lang.to_string();
            let mut code = String::new();
            let mut rest = Vec::new();
            for e in iter.by_ref() {
                match e {
                    Event::End(TagEnd::CodeBlock) => break,
                    Event::Text(t) => code.push_str(&t),
                    other => rest.push(other),
                }
            }
            let replaced = if lang == "mermaid" {
                Some(format!("<pre class=\"mermaid\">{}</pre>\n", esc(&code)))
            } else if lang.is_empty() {
                None
            } else {
                theme.and_then(|t| highlight(&code, &lang, t))
            };
            match replaced {
                Some(h) => out_events.push(Event::Html(CowStr::from(h))),
                None => {
                    out_events.push(ev);
                    out_events.push(Event::Text(CowStr::from(code)));
                    out_events.extend(rest);
                    out_events.push(Event::End(TagEnd::CodeBlock));
                }
            }
            continue;
        }
        out_events.push(ev);
    }

    let mut body = String::new();
    html::push_html(&mut body, out_events.into_iter());

    if body.contains("[toc]") {
        let list = toc_html(&toc);
        body = body.replace("<p>[toc]</p>", &list).replace("[toc]", &list);
    }
    Rendered {
        html: body,
        toc: Value::Array(toc),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Nested `<ul>` table of contents.
pub fn toc_html(toc: &[Value]) -> String {
    if toc.is_empty() {
        return String::new();
    }
    let min = toc
        .iter()
        .filter_map(|t| t["lvl"].as_u64())
        .min()
        .unwrap_or(1);
    let mut out = String::from("<ul class=\"toc\">");
    let mut depth = min;
    let mut open_li = false;
    for t in toc {
        let lvl = t["lvl"].as_u64().unwrap_or(min).max(min);
        while depth < lvl {
            out.push_str("<ul>");
            depth += 1;
            open_li = false;
        }
        while depth > lvl {
            out.push_str("</li></ul>");
            depth -= 1;
        }
        if open_li {
            out.push_str("</li>");
        }
        out.push_str(&format!(
            "<li><a href=\"#{}\">{}</a>",
            esc(t["slug"].as_str().unwrap_or_default()),
            esc(t["content"].as_str().unwrap_or_default())
        ));
        open_li = true;
    }
    while depth > min {
        out.push_str("</li></ul>");
        depth -= 1;
    }
    out.push_str("</li></ul>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter() {
        let d = parse_front_matter(
            "---\ntitle: 'Hi'\ndate: 2024-01-02\ntags: [a, b]\n---\n\n# Body",
            "x",
        )
        .unwrap();
        assert_eq!(d.front_matter["title"], "Hi");
        assert_eq!(d.front_matter["date"], "2024-01-02");
        assert_eq!(d.body, "# Body");
        assert!(parse_front_matter("---\n: [\n---\n", "x").is_err());
        assert_eq!(parse_front_matter("no fm", "x").unwrap().body, "no fm");
    }

    #[test]
    fn code_highlighting() {
        let r = render(
            "```rust\nfn main() {}\n```\n\n```mermaid\ngraph TD; A-->B\n```\n\n```\nplain\n```",
        );
        assert!(
            r.html.contains("<pre data-lang=\"rust\" style="),
            "{}",
            r.html
        );
        assert!(r.html.contains("<pre class=\"mermaid\">graph TD; A--&gt;B"));
        assert!(r.html.contains("<pre><code>plain"));
        let plain = render_with("```rust\nfn x(){}\n```", None);
        assert!(plain.html.contains("<code class=\"language-rust\">"));
    }

    #[test]
    fn headings_and_toc() {
        let r =
            render("[toc]\n\n# Hello World\n\n## Hello World\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(r.html.contains("<h1 id=\"hello-world\">"), "{}", r.html);
        assert!(r.html.contains("<h2 id=\"hello-world-1\">"));
        assert!(r.html.contains("<table>"));
        assert!(r.html.contains("<a href=\"#hello-world-1\">"));
        assert_eq!(r.toc.as_array().unwrap().len(), 2);
    }
}

//! `statico docs` and `statico explain`: the reference ships inside the binary,
//! so humans and agents can read it offline and version-matched.

pub const REFERENCE: &str = include_str!("../docs/REFERENCE.md");

/// `(topic, body)` pairs from the `## topic` sections of the reference
/// (headings inside code fences are ignored).
pub fn topics() -> Vec<(&'static str, &'static str)> {
    let mut starts: Vec<(usize, &'static str)> = Vec::new();
    let mut in_fence = false;
    let mut offset = 0;
    for line in REFERENCE.split_inclusive('\n') {
        if line.starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence && let Some(name) = line.strip_prefix("## ") {
            starts.push((offset, name.trim()));
        }
        offset += line.len();
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, (start, name))| {
            let body_start = start + REFERENCE[*start..].find('\n').map(|n| n + 1).unwrap_or(0);
            let end = starts
                .get(i + 1)
                .map(|(s, _)| *s)
                .unwrap_or(REFERENCE.len());
            (*name, REFERENCE[body_start..end].trim())
        })
        .collect()
}

/// One-line summaries for `statico docs`.
pub fn summary(topic: &str) -> &'static str {
    match topic {
        "project" => "folders and what goes where",
        "cli" => "every command and flag",
        "layouts" => "<layout src> and {slot}",
        "includes" => "<include src> partials with attributes",
        "placeholders" => "{frontmatter.x}, {site.x}, {seo}, { url('/x') } …",
        "filters" => "{post.date|date:\"%B %Y\"}, truncate, default, slug …",
        "loops" => "<ForEach> over collections and markdown",
        "conditions" => "<If>, <ElseIf>, <Else> and the expression syntax",
        "content" => "markdown pages, front matter, drafts, [toc]",
        "collections" => "JSON / YAML / TOML / CSV data files",
        "pagination" => "paginate=\"10\" and {pagination.*}",
        "taxonomies" => "tag and category pages",
        "feeds" => "Atom feeds",
        "seo" => "meta, Open Graph, Twitter, JSON-LD",
        "build" => "what `statico build` produces; Tailwind and esbuild",
        "config" => "statico.json",
        "schemas" => "validate front matter",
        "agents" => "using Statico from AI coding agents",
        "errors" => "error codes",
        _ => "",
    }
}

pub fn topic(name: &str) -> Option<String> {
    let name = name.trim().to_lowercase();
    let alias = match name.as_str() {
        "foreach" | "loop" | "collection" => "loops",
        "if" | "condition" | "else" => "conditions",
        "layout" => "layouts",
        "include" => "includes",
        "markdown" | "posts" | "frontmatter" => "content",
        "tags" | "taxonomy" | "categories" => "taxonomies",
        "rss" | "atom" | "feed" => "feeds",
        "json" | "mcp" | "ai" | "llms" => "agents",
        "codes" | "error" => "errors",
        "statico.json" | "static.json" => "config",
        n => n,
    }
    .to_string();
    topics()
        .into_iter()
        .find(|(t, _)| *t == alias)
        .map(|(t, b)| format!("## {t}\n\n{b}\n"))
}

pub struct Code {
    pub code: &'static str,
    pub title: &'static str,
    pub fix: &'static str,
}

pub const CODES: &[Code] = &[
    Code {
        code: "S001",
        title: "A <ForEach> or <If> is not closed, or tags are mismatched.",
        fix: "Every <ForEach> needs </ForEach> and every <If> needs </If>. <ElseIf>/<Else> must sit directly inside an <If>. The error names the line where the unclosed tag opened.",
    },
    Code {
        code: "S002",
        title: "An include, layout or page file was not found.",
        fix: "Paths are relative to includes/, layouts/ or pages/: <include src=\"nav.html\"> reads includes/nav.html. Check the spelling, or create it with `statico add include nav`.",
    },
    Code {
        code: "S003",
        title: "<layout> or <include> has no src attribute.",
        fix: "Add src, e.g. <layout src=\"main.html\" title=\"…\">.",
    },
    Code {
        code: "S004",
        title: "A condition or where expression could not be parsed.",
        fix: "Use ==, !=, <, >, &&, ||, !, contains, parentheses and quoted strings: condition=\"post.tags contains 'rust' && loop > 1\". See `statico docs conditions`.",
    },
    Code {
        code: "S005",
        title: "Front matter is not valid YAML.",
        fix: "Front matter sits between two --- lines at the very top. Quote values containing colons: title: \"Rust: a story\".",
    },
    Code {
        code: "S006",
        title: "A collection is missing or not valid data.",
        fix: "<ForEach collection=\"menu\"> reads collections/menu.json (or .yaml/.toml/.csv); content=\"posts\" reads content/posts/*.md. Validate the file's syntax.",
    },
    Code {
        code: "S007",
        title: "A markdown file has no page template.",
        fix: "content/blog/x.md needs pages/blog/[content].html (or pages/blog/x.html / pages/blog/index.html). `statico add post blog \"Title\"` creates one if missing.",
    },
    Code {
        code: "S008",
        title: "An include or layout includes itself (directly or indirectly).",
        fix: "Follow the chain printed in the error and break the cycle.",
    },
    Code {
        code: "S101",
        title: "A link points to a page or file that the build does not produce.",
        fix: "Fix the href, create the page (`statico add page …`), or put the file in public/ or assets/. `statico routes` lists every URL.",
    },
    Code {
        code: "S102",
        title: "An <img> has no alt attribute.",
        fix: "Describe the image: alt=\"Team photo at the 2024 offsite\". Use alt=\"\" only for purely decorative images.",
    },
    Code {
        code: "S103",
        title: "A page has no <title> (or an empty one).",
        fix: "Give the layout a <title>{title}</title> and pass title=\"…\" from the page's <layout> tag.",
    },
    Code {
        code: "S104",
        title: "A page has no meta description.",
        fix: "Add {seo} to the layout <head> and a description in front matter, or `site.description` in statico.json.",
    },
    Code {
        code: "S105",
        title: "A link points to an #anchor that does not exist on the target page.",
        fix: "Heading ids are generated GitHub-style: \"## Getting started\" → #getting-started. Check the id or use ## Title {#custom-id}.",
    },
    Code {
        code: "S106",
        title: "A placeholder was not replaced, usually a typo.",
        fix: "Check the variable name, e.g. {frontmatter.titel} → {frontmatter.title}. Placeholders only resolve where their data exists (frontmatter only on markdown pages).",
    },
    Code {
        code: "S107",
        title: "<html> has no lang attribute.",
        fix: "Add it to your layout: <html lang=\"en\">. Screen readers and translators rely on it.",
    },
    Code {
        code: "S108",
        title: "The value of a private .env key appears in the build output.",
        fix: "Something copied a secret into a page, feed or search index: a hard-coded value, a collection, or content pasted from .env. Remove it, and rotate the secret if the site was already deployed. Only keys starting with PUBLIC_ are meant to be published.",
    },
    Code {
        code: "S109",
        title: "A template reads a .env key that is not public.",
        fix: "Templates only see .env keys that start with PUBLIC_, so `.env` can hold API keys safely. If the value is fine to publish, rename it: API_URL=… → PUBLIC_API_URL=… and use {env.PUBLIC_API_URL}.",
    },
    Code {
        code: "S201",
        title: "Front matter does not match the schema for its folder.",
        fix: "Compare the file with `schemas` in statico.json. Types: string, number, bool, date, list, url, any; a trailing ? makes a field optional.",
    },
];

pub fn explain(code: &str) -> Option<&'static Code> {
    let c = code.trim().to_uppercase();
    CODES.iter().find(|x| x.code == c)
}

/// JSON Schema for statico.json.
pub const SCHEMA: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "statico.json",
  "type": "object",
  "properties": {
    "$schema": { "type": "string" },
    "site": {
      "type": "object",
      "description": "Site-wide data, available as {site.*}. name/description/image/author/language/twitter feed {seo}, feeds and llms.txt.",
      "properties": {
        "name": { "type": "string" }, "description": { "type": "string" }, "image": { "type": "string" },
        "author": { "type": "string" }, "language": { "type": "string" }, "twitter": { "type": "string" }
      }
    },
    "build": {
      "type": "object",
      "properties": {
        "url": { "type": "string", "description": "Absolute base URL, e.g. https://example.com" },
        "directory": { "type": "string", "default": "_site" },
        "minify": { "type": "boolean", "default": true },
        "hash_assets": { "type": "boolean", "default": true },
        "images": { "type": "boolean", "default": true },
        "sitemap": { "type": "boolean", "default": true },
        "llms": { "type": "boolean", "default": true },
        "markdown_mirrors": { "type": "boolean", "default": true },
        "search": { "oneOf": [{ "type": "boolean" }, { "const": "pagefind" }], "default": false }
      },
      "additionalProperties": false
    },
    "dev": {
      "type": "object",
      "properties": { "url": { "type": "string" }, "port": { "type": "integer" } },
      "additionalProperties": false
    },
    "markdown": {
      "type": "object",
      "properties": { "highlight": { "oneOf": [{ "type": "string" }, { "const": false }] } }
    },
    "feeds": {
      "oneOf": [
        { "const": false },
        { "type": "array", "items": { "type": "object", "required": ["content"],
          "properties": { "content": { "type": "string" }, "path": { "type": "string" },
                          "title": { "type": "string" }, "limit": { "type": "integer" } } } }
      ]
    },
    "schemas": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "additionalProperties": { "type": "string", "pattern": "^(string|text|number|bool|boolean|date|list|array|url|any)\\??$" }
      }
    },
    "headers": { "type": "object", "additionalProperties": { "type": "string" } }
  }
}
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_parse() {
        let t = topics();
        assert!(t.len() > 10);
        assert!(topic("loops").unwrap().contains("paginate"));
        assert!(topic("foreach").is_some());
        assert!(explain("s101").is_some());
        // Every code in the reference table has an explanation.
        for line in REFERENCE
            .lines()
            .filter(|l| l.starts_with("| F") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        {
            let code = &line[2..6];
            assert!(explain(code).is_some(), "{code}");
        }
        serde_json::from_str::<serde_json::Value>(SCHEMA).unwrap();
    }
}

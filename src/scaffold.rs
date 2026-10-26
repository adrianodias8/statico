//! `forno init`, `forno add …` and `forno agents`: create files in the right
//! place with the right boilerplate (handy for humans, essential for agents).

use anyhow::{Result, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::template::filters::slugify;

fn write_new(root: &Path, rel: &str, content: &str, created: &mut Vec<String>) -> Result<()> {
    let p = root.join(rel);
    if p.exists() {
        bail!("{rel} already exists");
    }
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&p, content)?;
    created.push(rel.to_string());
    Ok(())
}

fn write_if_missing(
    root: &Path,
    rel: &str,
    content: &str,
    created: &mut Vec<String>,
) -> Result<()> {
    if root.join(rel).exists() {
        return Ok(());
    }
    write_new(root, rel, content, created)
}

/// The layout new pages should use: `main.html` if present, else the first one.
pub fn default_layout(root: &Path) -> Option<String> {
    let dir = root.join("layouts");
    if dir.join("main.html").is_file() {
        return Some("main.html".into());
    }
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".html"))
        .collect();
    names.sort();
    names.into_iter().next()
}

fn html_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

fn title_from(name: &str) -> String {
    let base = name
        .rsplit('/')
        .next()
        .unwrap_or(name)
        .replace(['-', '_'], " ");
    let mut c = base.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn add_page(
    root: &Path,
    name: &str,
    title: Option<&str>,
    layout: Option<&str>,
) -> Result<Vec<String>> {
    let name = name.trim_matches('/').trim_end_matches(".html");
    if name.is_empty() || name.contains("..") {
        bail!("invalid page name `{name}`");
    }
    let title = title
        .map(str::to_string)
        .unwrap_or_else(|| title_from(name));
    let layout = layout.map(str::to_string).or_else(|| default_layout(root));
    let body = match layout {
        Some(l) => format!(
            "<layout src=\"{l}\" title=\"{}\">\n\n    <main>\n        <h1>{}</h1>\n    </main>\n\n</layout>\n",
            html_attr(&title),
            title
        ),
        None => format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n    <meta charset=\"UTF-8\">\n    <title>{title}</title>\n</head>\n<body>\n    <h1>{title}</h1>\n</body>\n</html>\n"
        ),
    };
    let mut created = Vec::new();
    write_new(root, &format!("pages/{name}.html"), &body, &mut created)?;
    Ok(created)
}

pub fn add_post(
    root: &Path,
    folder: &str,
    title: &str,
    draft: bool,
    schema: Option<&Value>,
) -> Result<Vec<String>> {
    let folder = folder.trim_matches('/');
    if folder.is_empty() || folder.contains("..") {
        bail!("invalid content folder `{folder}`");
    }
    let slug = slugify(title);
    if slug.is_empty() {
        bail!("the title must contain letters or digits");
    }
    let today = chrono::Local::now().format("%Y-%m-%d");
    let mut fm = format!(
        "title: \"{}\"\ndate: {today}\ndescription: \"\"\n",
        title.replace('"', "\\\"")
    );
    // Pre-fill other required fields from the folder's schema.
    if let Some(Value::Object(schema)) = schema {
        for (field, ty) in schema {
            if ["title", "date", "description"].contains(&field.as_str()) {
                continue;
            }
            let ty = ty.as_str().unwrap_or("any");
            let placeholder = match ty.trim_end_matches('?') {
                "list" | "array" => "[]",
                "bool" | "boolean" => "false",
                "number" => "0",
                "date" => "",
                _ => "\"\"",
            };
            if !ty.ends_with('?') {
                fm.push_str(&format!("{field}: {placeholder}\n"));
            }
        }
    }
    if draft {
        fm.push_str("draft: true\n");
    }
    let mut created = Vec::new();
    write_new(
        root,
        &format!("content/{folder}/{slug}.md"),
        &format!("---\n{fm}---\n\nWrite your post here.\n"),
        &mut created,
    )?;

    // Make sure the folder has a template and a listing page.
    let site_pages = root.join("pages").join(folder);
    let has_template = ["[content].html", "index.html"]
        .iter()
        .any(|f| site_pages.join(f).is_file());
    if !has_template && let Some(layout) = default_layout(root) {
        write_if_missing(
            root,
            &format!("pages/{folder}/[content].html"),
            &format!(
                "<layout src=\"{layout}\" title=\"{{frontmatter.title}}\">\n\n    <article>\n        <h1>{{frontmatter.title}}</h1>\n        <time datetime=\"{{frontmatter.date}}\">{{frontmatter.date|date}}</time>\n        {{content}}\n    </article>\n\n</layout>\n"
            ),
            &mut created,
        )?;
        write_if_missing(
            root,
            &format!("pages/{folder}/index.html"),
            &format!(
                "<layout src=\"{layout}\" title=\"{}\">\n\n    <main>\n        <h1>{}</h1>\n        <ForEach content=\"{folder}\" as=\"post\" orderBy=\"date, desc\" paginate=\"10\">\n            <article>\n                <h2><a href=\"{{post.url}}\">{{post.title}}</a></h2>\n                <time datetime=\"{{post.date}}\">{{post.date|date}}</time>\n                <p>{{post.description}}</p>\n            </article>\n        </ForEach>\n        <nav>\n            <If condition=\"pagination.has_prev\"><a href=\"{{pagination.prev_url}}\">Newer</a></If>\n            <If condition=\"pagination.has_next\"><a href=\"{{pagination.next_url}}\">Older</a></If>\n        </nav>\n    </main>\n\n</layout>\n",
                title_from(folder),
                title_from(folder)
            ),
            &mut created,
        )?;
    }
    Ok(created)
}

pub fn add_include(root: &Path, name: &str) -> Result<Vec<String>> {
    let name = name.trim_matches('/').trim_end_matches(".html");
    let mut created = Vec::new();
    write_new(
        root,
        &format!("includes/{name}.html"),
        &format!(
            "<!-- includes/{name}.html — use with <include src=\"{name}.html\" />; attributes become {{name}} placeholders -->\n<div></div>\n"
        ),
        &mut created,
    )?;
    Ok(created)
}

pub const LAYOUT: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    {seo}
    {tailwindcss}
</head>
<body>
    {slot}
</body>
</html>
"#;

pub fn add_layout(root: &Path, name: &str) -> Result<Vec<String>> {
    let name = name.trim_matches('/').trim_end_matches(".html");
    let mut created = Vec::new();
    write_new(root, &format!("layouts/{name}.html"), LAYOUT, &mut created)?;
    Ok(created)
}

pub fn add_collection(root: &Path, name: &str) -> Result<Vec<String>> {
    let name = name.trim_matches('/').trim_end_matches(".json");
    let mut created = Vec::new();
    write_new(
        root,
        &format!("collections/{name}.json"),
        "[\n    { \"title\": \"First item\" }\n]\n",
        &mut created,
    )?;
    Ok(created)
}

pub fn agents_md(root: &Path) -> String {
    let layouts = std::fs::read_dir(root.join("layouts"))
        .map(|d| {
            let mut v: Vec<String> = d
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            v.sort();
            v.join(", ")
        })
        .unwrap_or_default();
    format!(
        r#"# AGENTS.md

This is a static website built with **Forno** (plain HTML + a few tags).
Run `forno docs --all` for the complete, version-matched reference.

## Commands

- `forno dev` — dev server with live reload on http://localhost:3000
- `forno check --json` — build + verify links, anchors, alt text, titles. **Done means zero errors.**
- `forno build` — production build into `_site/` (never edit `_site/` by hand)
- `forno routes --json` — every URL and the file that renders it
- `forno add page <path>` / `forno add post <folder> "<title>"` / `forno add include <name>`
- `forno explain <code>` — explain an error code such as F101
- Add `--json` to any command for machine-readable output; exit code 1 means errors.

## Where things live

- `pages/` — one HTML file per URL (`pages/about.html` → `/about/`)
- `layouts/` — page shells{layouts_list}; pages use `<layout src="main.html" title="…">…</layout>`
- `includes/` — partials: `<include src="nav.html" />`; attributes become `{{name}}`
- `content/<folder>/*.md` — markdown with YAML front matter, rendered by `pages/<folder>/[content].html`
- `collections/` — JSON/YAML/TOML/CSV data for `<ForEach collection="name">`
- `assets/css/main.css` (Tailwind), `assets/js/main.js` (bundled), `assets/images/`
- `public/` — copied to the site root as-is
- `forno.json` — optional config (`forno schema` prints its JSON Schema)

## Template cheatsheet

```html
<ForEach content="posts" as="post" orderBy="date, desc" paginate="10" where="!post.draft">
  <a href="{{post.url}}">{{post.title}}</a> {{post.date|date:"%B %-d, %Y"}}
</ForEach>
<If condition="post.tags contains 'news'">…<ElseIf condition="loop == 1">…<Else>…</If>
{{seo}} {{tailwindcss}} {{content}} {{frontmatter.title}} {{site.name}} {{page.url}} {{ url('/about') }}
```

## Rules

- Use plain HTML and Tailwind classes; do not add a JavaScript framework or build tool.
- Keep shared markup in `includes/` and `layouts/` instead of duplicating it.
- Every `<img>` needs a meaningful `alt`.
- After changing anything, run `forno check --json` and fix every error before finishing.
"#,
        layouts_list = if layouts.is_empty() {
            String::new()
        } else {
            format!(" ({layouts})")
        }
    )
}

pub fn write_agents(root: &Path) -> Result<Vec<String>> {
    let mut created = Vec::new();
    let agents = root.join("AGENTS.md");
    let existed = agents.exists();
    std::fs::write(&agents, agents_md(root))?;
    created.push(if existed {
        "AGENTS.md (updated)".into()
    } else {
        "AGENTS.md".into()
    });
    write_if_missing(
        root,
        "CLAUDE.md",
        "# CLAUDE.md\n\nSee @AGENTS.md for how this Forno site is organised and how to verify changes.\n",
        &mut created,
    )?;
    Ok(created)
}

/// `forno init`: a minimal project in an existing (usually empty) folder.
pub fn init(root: &Path, name: Option<&str>) -> Result<Vec<String>> {
    if root.join("pages").exists() {
        bail!("{} already has a pages/ folder", root.display());
    }
    std::fs::create_dir_all(root)?;
    let site_name = name.map(str::to_string).unwrap_or_else(|| {
        root.canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| title_from(&n.to_string_lossy())))
            .unwrap_or_else(|| "My Site".into())
    });
    let mut created = Vec::new();
    let w = |rel: &str, c: &str, created: &mut Vec<String>| write_if_missing(root, rel, c, created);
    w(
        "forno.json",
        &format!(
            "{{\n    \"site\": {{\n        \"name\": {},\n        \"description\": \"\",\n        \"language\": \"en\"\n    }}\n}}\n",
            serde_json::to_string(&site_name)?
        ),
        &mut created,
    )?;
    w("layouts/main.html", LAYOUT, &mut created)?;
    w(
        "includes/header.html",
        "<header class=\"mx-auto max-w-3xl px-6 py-6 flex items-center justify-between\">\n    <a href=\"/\" class=\"font-semibold\">{site.name}</a>\n    <nav class=\"flex gap-4 text-sm\">\n        <a href=\"/\">Home</a>\n    </nav>\n</header>\n",
        &mut created,
    )?;
    w(
        "pages/index.html",
        "<layout src=\"main.html\" title=\"{site.name}\">\n\n    <include src=\"header.html\" />\n\n    <main class=\"mx-auto max-w-3xl px-6 py-16\">\n        <h1 class=\"text-4xl font-bold tracking-tight\">Hello from Forno 🔥</h1>\n        <p class=\"mt-4 text-lg text-neutral-600\">Edit <code>pages/index.html</code> and save — the page reloads.</p>\n    </main>\n\n</layout>\n",
        &mut created,
    )?;
    w(
        "pages/404.html",
        "<layout src=\"main.html\" title=\"Not found\">\n    <main class=\"mx-auto max-w-3xl px-6 py-24 text-center\">\n        <h1 class=\"text-3xl font-bold\">Page not found</h1>\n        <p class=\"mt-4\"><a class=\"underline\" href=\"/\">Back home</a></p>\n    </main>\n</layout>\n",
        &mut created,
    )?;
    w(
        "assets/css/main.css",
        "@import \"tailwindcss\";\n\n/* Your custom CSS */\n",
        &mut created,
    )?;
    w(
        "public/robots.txt",
        "User-agent: *\nAllow: /\n",
        &mut created,
    )?;
    w("content/.gitkeep", "", &mut created)?;
    w("collections/.gitkeep", "", &mut created)?;
    w(".gitignore", "_site/\nnode_modules/\n.env\n", &mut created)?;
    created.extend(write_agents(root)?);
    Ok(created)
}

pub fn rel_paths(root: &Path, files: &[String]) -> Vec<PathBuf> {
    files.iter().map(|f| root.join(f)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_add_and_build() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let created = init(root, Some("Test")).unwrap();
        assert!(created.contains(&"AGENTS.md".to_string()));
        add_page(root, "about/team", Some("Our team"), None).unwrap();
        let post = add_post(root, "blog", "Hello, World!", false, None).unwrap();
        assert!(post.contains(&"content/blog/hello-world.md".to_string()));
        assert!(post.contains(&"pages/blog/[content].html".to_string()));
        add_include(root, "card").unwrap();
        assert!(
            add_page(root, "about/team", None, None).is_err(),
            "no overwrite"
        );
        let page = std::fs::read_to_string(root.join("pages/about/team.html")).unwrap();
        assert!(page.contains("<layout src=\"main.html\" title=\"Our team\">"));
        let agents = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(agents.contains("forno check --json") && agents.contains("main.html"));
    }
}

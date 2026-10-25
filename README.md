# ⚡ Static (Rust)

A fast, single-binary static site generator — a Rust rewrite of
[DevDojo's Static](https://github.com/thedevdojo/static). Your existing Static
projects build unchanged; pages, layouts, includes, collections and markdown
content work exactly as before, with the original's bugs fixed and a few
well-earned extras. See [PLAN.md](PLAN.md) for the audit and design notes.

- **One binary.** No Node.js needed to render your site.
- **~10× faster builds**, parallel page rendering, sub-millisecond dev renders.
- **Tailwind that actually works**: v3 (`@tailwind base;` + `tailwind.config.js`, used by every official template) and v4 (`@import "tailwindcss";`) are detected automatically.
- **Real errors**: `pages/blog/index.html:12: <ForEach> is never closed`, shown in the terminal *and* in the browser.

## Install

```sh
cargo install --path .          # from this folder
# the binary is called `static`
```

## Commands

```sh
static new my-site                 # starter template
static new my-blog --blog          # or --template blog | owner/repo | ./folder | file.zip
static dev                         # http://localhost:3000, live reload
static dev --port 8080 --open
static build                       # → _site/
static build https://example.com   # absolute URLs + sitemap.xml
static serve                       # preview _site/ like a static host
static templates                   # list official templates
```

Use `-C <dir>` to run against another folder.

## Project layout

```
pages/        routes: pages/about.html → /about/,  pages/blog/index.html → /blog/
layouts/      <layout src="main.html" title="…"> … </layout>  →  {slot}, {title}
includes/     <include src="nav.html" active="home"></include>  →  {active}
collections/  menu.json → <ForEach collection="menu">{menu.title}</ForEach>
content/      markdown: content/blog/hello.md → /blog/hello/ via pages/blog/[content].html
assets/       css/main.css (Tailwind), js/main.js (bundled with esbuild), images/…
public/       copied to the site root as-is (favicon.ico, robots.txt, CNAME…)
static.json   optional config
.env          optional, exposed as {env.KEY}
```

## Templates

### Layouts and includes

```html
<layout title="About" src="main.html">
    <include src="header.html" title="About us" />
    <p>Hello</p>
</layout>
```

A layout file can itself start with `<layout src="base.html">` to nest layouts.
Includes can contain includes. `{tailwindcss}` in a layout's `<head>` becomes the
compiled stylesheet in `build` and the in-browser compiler in `dev`.

### Loops

```html
<ForEach collection="menu" as="item">
    <a href="{item.link}">{item.title}</a>
</ForEach>

<ForEach content="posts" as="post" orderBy="date, desc" count="3" where="!post.hidden">
    <If condition="loop == 1"><h2>Latest</h2></If>
    <a href="{post.link}">{post.title}</a>
    <ForEach collection="post.tags" as="tag"><span>{tag}</span></ForEach>
</ForEach>
```

Attributes: `collection` / `content`, `as`, `orderBy="key[, desc]"`, `count`,
`offset`, `where`. Inside a loop: `{loop}` (1-based) and `{$loop.index}`,
`{$loop.first}`, `{$loop.last}`, `{$loop.count}`. Content items expose their
front matter plus `link`, `slug`, `content` (markdown) and `html` (rendered).

`{collections.menu.json}` inlines a collection as JSON (handy for Alpine.js).

### Conditions

```html
<If condition="frontmatter.featured && post.tags contains 'rust'">…
<ElseIf condition="loop > 1">…
<Else>…
</If>
```

Supports `== != === !== < <= > >=`, `&& || !` (or `and or not`), `contains`,
`.length`, parentheses, strings, numbers and `{placeholder}` syntax. Conditions
are evaluated by a small safe interpreter — never as JavaScript.

### Markdown content

```markdown
---
title: Hello
date: 2024-05-01
draft: true      # shown in `dev`, skipped in `build`
---
[toc]

## Section
```

The page template (`pages/blog/[content].html`) gets `{content}`,
`{frontmatter.*}`, `{static_content_element}`, and the
`window.toc` / `window.frontmatter` globals.

### Other placeholders

| Placeholder | Value |
|---|---|
| `{ url('/path') }` | `/path`, or `https://example.com/path` when building with a URL |
| `{page.url}` | current URL, e.g. `/blog/` — useful for active nav links |
| `{site.name}` | anything under `"site"` in static.json |
| `{env.KEY}` | from `.env` only (your shell environment is never exposed) |

## static.json

```json
{
  "site": { "name": "My Site" },
  "build": { "url": "https://example.com", "directory": "_site", "sitemap": true },
  "dev": { "port": 3000 },
  "headers": { "X-Frame-Options": "DENY" }
}
```

## Tailwind and JavaScript

Rendering never needs Node. For CSS and JS, `static build` looks for, in order:
`STATIC_TAILWIND` / `STATIC_ESBUILD` env overrides → `node_modules/.bin` → a
standalone binary on `PATH` → `npx`. If nothing is available the build still
succeeds (JS copied as-is, CSS via the in-browser compiler) and tells you so.
`--skip-tools` skips them explicitly.

## Deploying (GitHub Pages)

```yaml
- run: cargo install --git https://github.com/<you>/static-rs
- run: static build https://<you>.github.io
- uses: actions/upload-pages-artifact@v3
  with: { path: ./_site }
```

## Development

```sh
cargo test                # unit + end-to-end fixture tests
cargo clippy --all-targets
```

## License

ISC, like the original. Static was created by [Tony Lea](https://github.com/tnylea) / DevDojo;
the built-in 404 page is theirs.

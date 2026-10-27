# Statico reference

Statico turns plain HTML into static sites. Everything is HTML files in a few
folders; there is no build config to write and no template language to learn
beyond a handful of tags.

## project

```
pages/        routes          pages/about.html → /about/   pages/blog/index.html → /blog/
layouts/      page shells     <layout src="main.html" title="…"> … </layout>
includes/     partials        <include src="nav.html" />
collections/  data files      menu.json | .yaml | .toml | .csv
content/      markdown        content/blog/hello.md → /blog/hello/
assets/       css, js, images (css/main.css → Tailwind, js/main.js → esbuild)
public/       copied to the site root as-is (favicon.ico, robots.txt, CNAME)
statico.json    optional config (static.json also works)
.env          optional; PUBLIC_ keys available as {env.PUBLIC_KEY}
```

Output goes to `_site/`. Never edit `_site/` by hand; it is rebuilt from scratch.

Special pages:
- `pages/404.html` → `/404.html` (what static hosts serve for missing pages)
- `pages/<folder>/[content].html` → template for every `content/<folder>/*.md`
- `pages/<folder>/[tag].html` → one page per tag (see `taxonomies`)

## cli

```
statico new <folder> [--template starter|blog|aria|boxed|stone|owner/repo|./dir|file.zip]
statico init                     turn the current folder into a Statico project
statico dev [--port 3000] [--open]
statico build [url] [--out dir] [--no-minify] [--skip-tools]
statico serve                    preview the built site like a static host
statico check [--strict]         build in a temp dir and check links, anchors, alt text, titles
statico add page|post|include|layout|collection <name> [--title …]
statico routes                   list every URL and the file that renders it
statico docs [topic|--all]       this reference, offline
statico explain <code>           explain an error code, e.g. S101
statico diff <old_dir> <new_dir> compare two builds (safe upgrades)
statico schema                   JSON Schema for statico.json
statico agents                   write AGENTS.md / CLAUDE.md for AI coding agents
statico templates                list starter templates
```

Global flags: `--json` (one machine-readable JSON object on stdout),
`-C <dir>` (run in another folder). Exit codes: `0` success, `1` errors
(build failed, check found problems), `2` invalid usage.

## layouts

A page picks a layout and passes attributes:

```html
<layout src="main.html" title="About us">
    <h1>About</h1>
</layout>
```

`layouts/main.html` receives the page body in `{slot}` and each attribute as
`{name}`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <title>{title}</title>
    {seo}
    {tailwindcss}
</head>
<body>{slot}</body>
</html>
```

A layout may itself start with `<layout src="base.html">` (nested layouts).

## includes

```html
<include src="card.html" title="Hello" />
<include src="card.html" title="Hello"></include>
```

`includes/card.html` sees attributes as `{title}`. Includes can contain
includes (cycles are detected) and work in pages, layouts and markdown.

## placeholders

`{path}` is replaced when `path` is a known variable; anything else (like
`x-data="{open: true}"`) is left alone.

| Placeholder | Value |
|---|---|
| `{frontmatter.title}` | front matter of the current markdown page |
| `{content}` | rendered markdown of the current page |
| `{page.url}`, `{page.title}`, `{page.description}` | current page |
| `{site.name}` | anything under `site` in statico.json |
| `{env.PUBLIC_KEY}` | `.env` keys starting with `PUBLIC_` — never other keys, never your shell environment |
| `{collections.menu.json}` | a collection inlined as JSON |
| `{ url('/path') }` | `/path`, or absolute when building with a URL |
| `{seo}` | meta description, canonical, Open Graph, Twitter, JSON-LD, feed links |
| `{tailwindcss}` | compiled stylesheet (build) or in-browser Tailwind (dev) |
| `{static_content_element}` | hidden element with toc/front matter (Static compatibility) |

## filters

`{post.date|date:"%B %-d, %Y"}` — filters chain left to right.

| Filter | Example | Result |
|---|---|---|
| `date[:fmt]` | `{post.date|date:"%d %b %Y"}` | `02 Mar 2024` (strftime) |
| `upper`, `lower`, `capitalize` | `{site.name|upper}` | |
| `truncate[:n]` | `{post.excerpt|truncate:120}` | cuts at a word, adds `…` |
| `default:"x"` | `{post.subtitle|default:"—"}` | fallback for empty values |
| `slug` | `{post.title|slug}` | `hello-world` |
| `escape` | `{post.title|escape}` | HTML-escaped |
| `strip_html` | `{post.html|strip_html}` | plain text |
| `markdown` | `{item.body|markdown}` | renders markdown |
| `json` | `{post.tags|json}` | `["a","b"]` |
| `join[:sep]`, `first`, `last`, `length` | `{post.tags|join:", "}` | |
| `reading_time` | `{post.html|reading_time}` | minutes (220 wpm) |
| `abs_url` | `{post.url|abs_url}` | absolute URL when building with one |

Unknown filters leave the placeholder untouched, so typos are visible.

## loops

```html
<ForEach collection="menu" as="item">
    <a href="{item.link}">{item.title}</a>
</ForEach>
```

| Attribute | Meaning |
|---|---|
| `collection="menu"` | `collections/menu.json` (or .yaml/.toml/.csv) |
| `collection="post.tags"` | a list inside the current loop item (nested loops) |
| `collection="taxonomy/tags"` | all tags: `{name, slug, count, url}` |
| `content="posts"` | markdown files in `content/posts` |
| `as="post"` | loop variable name (default: the collection name) |
| `orderBy="date, desc"` | sort by a field |
| `where="post.featured && !post.draft"` | filter (same syntax as conditions) |
| `count="3"`, `offset="1"` | limit / skip |
| `paginate="10"` | 10 per page; the build writes `/page/2/`, … |

Inside a loop: `{loop}` (1-based index), `{$loop.index}`, `{$loop.index0}`,
`{$loop.first}`, `{$loop.last}`, `{$loop.count}`, `{$loop.total}`.

Content items have their front matter plus `link` (`/posts/hello`), `url`
(`/posts/hello/`), `slug`, `content` (markdown) and `html` (rendered).

## conditions

```html
<If condition="post.featured">★
<ElseIf condition="loop > 1 and post.tags contains 'rust'">…
<Else>…
</If>
```

Operators: `== != === !== < <= > >=`, `&& || !` (or `and or not`), `contains`
(substring, list membership, object key), parentheses, `.length`. Values:
numbers, `'strings'`, `true/false/null`, variables, `{placeholders}`. Inside a
loop, item fields can be used bare (`featured` = `post.featured`). Conditions
are interpreted safely; they never run JavaScript.

## content

`content/blog/hello.md`:

```markdown
---
title: Hello
date: 2024-05-01
description: Short summary used by SEO, feeds and llms.txt
tags: [rust, web]
draft: true        # visible in `statico dev`, excluded from `statico build`
---
[toc]

## Section
```

Rendered with the nearest template: `pages/blog/hello.html`, else
`pages/blog/[content].html`, else `pages/blog/index.html`, walking up.
Available there: `{content}`, `{frontmatter.*}`, `{toc}`.

Markdown supports tables, task lists, footnotes, strikethrough, heading ids
(`## Title {#custom-id}`), `[toc]`, syntax highlighting, and
` ```mermaid ` blocks (rendered as `<pre class="mermaid">`).

## collections

Data files in `collections/`: `.json`, `.yaml`/`.yml`, `.toml` (a single
array-of-tables becomes a list), `.csv` (numbers are detected).

## pagination

```html
<ForEach content="posts" as="post" orderBy="date, desc" paginate="10">…</ForEach>

<If condition="pagination.has_prev"><a href="{pagination.prev_url}">Newer</a></If>
Page {pagination.current} of {pagination.total}
<If condition="pagination.has_next"><a href="{pagination.next_url}">Older</a></If>
<ForEach collection="pagination.pages" as="p"><a href="{p.url}">{p.number}</a></ForEach>
```

`/blog/` is page 1; the build writes `/blog/page/2/`, `/blog/page/3/`, …

## taxonomies

Create `pages/tags/[tag].html`; Statico writes one page per distinct value of the
`tags` (or `tag`) front matter field, at `/tags/<slug>/`:

```html
<layout src="main.html" title="Posts tagged {tag}">
  <h1>{tag} ({term.count})</h1>
  <ForEach content="posts" as="post" where="post.tags contains tag">…</ForEach>
</layout>
```

Works for any field: `[category].html` uses `categories`/`category`.
List all terms anywhere with `<ForEach collection="taxonomy/tags" as="t">`.

## feeds

When the site has a URL (`statico build https://example.com` or `build.url`),
every top-level content folder whose posts have a `date` gets an Atom feed at
`/<folder>/feed.xml` (full content, newest 20). `{seo}` adds the
`<link rel="alternate">`. Configure or disable:

```json
{ "feeds": [{ "content": "posts", "path": "feed.xml", "title": "My blog", "limit": 50 }] }
{ "feeds": false }
```

Configured feeds are always written; without a URL their links are relative
(Statico warns, since most feed readers need absolute links).

## seo

Put `{seo}` in your layout's `<head>`. It emits meta description, canonical
URL, Open Graph and Twitter tags, `article:published_time`, feed links and
JSON-LD (`BlogPosting` for dated content, `WebSite` on the home page).

Sources: front matter `title`, `description`/`excerpt`, `image`/`ogImage`/
`coverImage`, `author`, `date`; fallbacks from `site` in statico.json:
`name`, `description`, `image`, `author`, `language`, `twitter`.

## build

`statico build` writes `_site/`:
- pages rendered in parallel, HTML minified, CSS/JS fingerprinted
  (`main.3f2a91c0.css`) with references rewritten
- local `<img>` get `width`/`height` (no layout shift), `loading="lazy"` and
  `decoding="async"` (the first image stays eager)
- `sitemap.xml` (with an absolute URL), Atom feeds
- `llms.txt`, `llms-full.txt` and a `.md` copy of every markdown page
  (`/blog/hello/` → `/blog/hello.md`) for AI agents
- `search.json` when `build.search` is `true`, or a Pagefind index with `"pagefind"`

Tailwind: `assets/css/main.css` is compiled with Tailwind v3 (`@tailwind base;`)
or v4 (`@import "tailwindcss";`), detected automatically. Statico uses, in order,
`STATICO_TAILWIND`, `node_modules/.bin`, a `tailwindcss` binary on PATH, a cached
standalone binary it downloads itself, then `npx`. JavaScript in
`assets/js/main.js` is bundled with esbuild the same way. No Node.js is required.
Downloaded binaries are checked against SHA-256 hashes pinned in Statico and
discarded, never run, if they don't match.

`.env`: like Vite and Astro, only keys that start with `PUBLIC_` reach
templates, so the same file can hold API keys for other tools. A template that
asks for any other key gets a warning (`S109`) and nothing is substituted.
`statico check` also fails (`S108`) if the value of a non-`PUBLIC_` key
(8 characters or longer) appears anywhere in the output: pages, feeds,
`search.json`, `llms-full.txt` or `.md` mirrors.

## config

`statico.json` (all keys optional; run `statico schema` for the JSON Schema):

```json
{
  "site": { "name": "My Site", "description": "…", "image": "/og.png", "author": "Me", "language": "en" },
  "build": {
    "url": "https://example.com", "directory": "_site",
    "minify": true, "hash_assets": true, "images": true,
    "sitemap": true, "llms": true, "markdown_mirrors": true, "search": false
  },
  "dev": { "port": 3000 },
  "markdown": { "highlight": "base16-ocean.dark" },
  "feeds": [{ "content": "posts" }],
  "schemas": { "posts": { "title": "string", "date": "date", "tags": "list?" } },
  "headers": { "X-Frame-Options": "DENY" }
}
```

Highlight themes: `base16-ocean.dark`, `base16-ocean.light`, `base16-eighties.dark`,
`base16-mocha.dark`, `InspiredGitHub`, `Solarized (dark)`, `Solarized (light)`,
or `false` to disable.

## schemas

Validate front matter per content folder. Types: `string`, `number`, `bool`,
`date`, `list`, `url`, `any`; add `?` for optional.

```json
{ "schemas": { "posts": { "title": "string", "date": "date", "tags": "list?" } } }
```

A mismatch fails the build with error `S201`, naming the file and field.

## agents

Statico is designed to be driven by AI coding agents through its CLI:

- `--json` on every command returns one JSON object; errors have `code`, `file`, `line`, `message`.
- `statico check --json` is the definition of done: zero errors.
- `statico routes --json` maps URLs to source files.
- `statico add …` creates files in the right place with the right boilerplate.
- `statico docs --all` prints this whole reference; `statico explain S101` explains an error.
- `statico agents` writes an `AGENTS.md` (and `CLAUDE.md`) describing the project conventions.
- The dev server answers `Accept: text/markdown` and `/<page>.md` with markdown.

## errors

| Code | Meaning |
|---|---|
| S001 | `<ForEach>`/`<If>` not closed or mismatched |
| S002 | include, layout or page file not found |
| S003 | `<layout>`/`<include>` without `src` |
| S004 | invalid condition or `where` expression |
| S005 | invalid front matter |
| S006 | collection missing or not valid data |
| S007 | no page template for a markdown file |
| S008 | include/layout cycle or nesting too deep |
| S101 | broken internal link (check) |
| S102 | image without `alt` (check) |
| S103 | page without `<title>` (check) |
| S104 | page without meta description (check, `--strict`) |
| S105 | link to a missing `#anchor` (check) |
| S106 | unresolved placeholder in output, e.g. a typo in `{frontmatter.titel}` (check) |
| S107 | `<html>` without a `lang` attribute (check) |
| S108 | the value of a private `.env` key appears in the output (check) |
| S109 | a template reads a `.env` key without the `PUBLIC_` prefix |
| S201 | front matter does not match its schema |

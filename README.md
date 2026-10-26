# 🔥 Forno

**Bake plain HTML into fast static sites.** One binary. No framework, no Node,
no template language to learn — just HTML files and a handful of tags.

```sh
forno new my-site        # or: mkdir my-site && cd my-site && forno init
cd my-site
forno dev                # http://localhost:3000, live reload
forno build https://my-site.com
```

Forno started as a Rust rewrite of [DevDojo's Static](https://github.com/thedevdojo/static)
and still builds Static projects unchanged (`static.json` works as-is).

## Why Forno

- **Plain HTML.** Pages are HTML files; layouts and includes are HTML files. Five tags: `<layout>`, `<include>`, `<ForEach>`, `<If>`, `<page>`.
- **One binary, no Node.** Tailwind (v3 and v4, auto-detected) and esbuild are fetched as standalone binaries on first use and cached.
- **Batteries included.** Markdown with syntax highlighting, pagination, tag pages, Atom feeds, SEO and Open Graph tags, sitemap, search index, image dimensions and lazy loading, minification, fingerprinted assets.
- **Errors that tell you where.** `pages/blog/index.html:12: <ForEach> is never closed [F001]` — in the terminal, in the browser, and as JSON.
- **Built for AI agents too.** `--json` on every command, `forno check` as a definition of done, `forno add` scaffolding, an `AGENTS.md` generator, `llms.txt` and markdown copies of every post.
- **Fast.** Pages render in parallel; a 1,000-post blog with syntax highlighting builds in ~1.5 s on a 2-core VM.

## Install

```sh
cargo install forno-cli        # binary: forno
```

Prebuilt binaries, a `curl | sh` installer, an npm wrapper and a Homebrew
formula are set up in [`packaging/`](packaging/) and publish with each tagged release.

## A site in two minutes

```
pages/index.html              →  /
pages/about.html              →  /about/
pages/blog/index.html         →  /blog/  (+ /blog/page/2/ …)
pages/blog/[content].html     →  template for content/blog/*.md
pages/tags/[tag].html         →  /tags/<tag>/ for every tag
content/blog/hello.md         →  /blog/hello/
layouts/main.html             →  page shell with {slot}
includes/header.html          →  <include src="header.html" />
collections/team.yaml         →  <ForEach collection="team">
assets/css/main.css           →  Tailwind, compiled and fingerprinted
public/                       →  copied as-is
```

```html
<!-- pages/blog/index.html -->
<layout src="main.html" title="Blog">
  <ForEach content="blog" as="post" orderBy="date, desc" paginate="10">
    <article>
      <a href="{post.url}">{post.title}</a>
      <time>{post.date|date:"%B %-d, %Y"}</time>
      <If condition="post.tags contains 'news'"><span>News</span></If>
    </article>
  </ForEach>
  <If condition="pagination.has_next"><a href="{pagination.next_url}">Older posts</a></If>
</layout>
```

```html
<!-- layouts/main.html -->
<!DOCTYPE html>
<html lang="en">
<head>
  <title>{title} · {site.name}</title>
  {seo}
  {tailwindcss}
</head>
<body>{slot}</body>
</html>
```

The full reference ships in the binary: `forno docs`, `forno docs loops`,
`forno docs --all`, or read [docs/REFERENCE.md](docs/REFERENCE.md).

## Commands

| Command | |
|---|---|
| `forno new <dir> [--template blog\|owner/repo\|./dir]` | new site from a template |
| `forno init` | turn the current folder into a site |
| `forno dev [--port] [--open]` | dev server, live reload, error overlay |
| `forno build [url] [--watch]` | production build into `_site/` |
| `forno serve` | preview the build like a static host |
| `forno check [--strict]` | broken links and anchors, alt text, titles, unresolved placeholders |
| `forno add page\|post\|include\|layout\|collection` | scaffold files |
| `forno routes` | every URL → source file |
| `forno docs [topic]` · `forno explain F101` | offline reference and error codes |
| `forno diff <old> <new>` | compare two builds (verify upgrades change nothing) |
| `forno agents` | write `AGENTS.md` + `CLAUDE.md` for coding agents |
| `forno schema` | JSON Schema for `forno.json` |

Every command accepts `--json` and `-C <dir>`. Exit code `1` means errors.

## Using Forno with AI coding agents

Agents work best with a terminal, plain files and fast feedback. Forno gives them:

1. **`AGENTS.md`** (from `forno new`, `forno init` or `forno agents`) — the project's conventions in one page.
2. **`forno check --json`** — structured errors with `code`, `file`, `url` and `message`; zero errors means done.
3. **`forno add …`** — files created in the right place with working boilerplate.
4. **`forno docs --all` / `forno explain`** — documentation that matches the installed version.
5. **Readable output for other AIs** — `llms.txt`, `llms-full.txt`, `/post.md` copies, and the dev server answers `Accept: text/markdown`.

There is no MCP server on purpose: agents that can run shell commands get
everything through the CLI, with fewer tokens. See [docs/RESEARCH.md](docs/RESEARCH.md).

## Coming from Static

Everything keeps working: `static.json`, `<layout>`, `<include>`, `<ForEach>`,
`<If>`, `{tailwindcss}`, `{ url() }`, `[content].html`, `window.toc`,
`{static_content_element}`, and `forno new my-site --blog`. Differences you'll
notice are fixes:

- Tailwind v3 projects (all official templates) now compile correctly.
- `<If>` conditions are evaluated safely instead of as JavaScript.
- `pages/404.html` builds to `/404.html`.
- Builds fail loudly, with every error listed, instead of half-succeeding.

## Project docs

- [docs/REFERENCE.md](docs/REFERENCE.md) — complete syntax and configuration
- [ROADMAP.md](ROADMAP.md) — what's done, what's next, and why
- [docs/RESEARCH.md](docs/RESEARCH.md) — what users of other generators are missing
- [packaging/README.md](packaging/README.md) — releases and distribution

## Development

```sh
cargo test                       # unit tests + end-to-end fixture
cargo clippy --all-targets -- -D warnings
```

## License

ISC. Forno began as a rewrite of Static by [Tony Lea](https://github.com/tnylea) / DevDojo.

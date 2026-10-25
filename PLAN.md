# Static in Rust — plan, audit and design notes

This is a rewrite of [thedevdojo/static](https://github.com/thedevdojo/static)
(`@devdojo/static` 1.1.0, ~1,350 lines of Node.js) as a single Rust binary that
builds existing Static projects unchanged.

## 1. Goals

1. **Drop-in compatible.** Every official template (`starter`, `blog`, …)
   builds with the same folder layout, tags and placeholders.
2. **One binary, no Node required** to render pages. Node/npx is only used —
   if present — for Tailwind and esbuild, exactly as before.
3. **Fix what's broken** in the original rather than port the bugs.
4. **Faster** builds and an instant dev loop.
5. **Better errors**: file + line, every failing page at once, in-browser overlay.

## 2. Audit of the original

| Area | Original behaviour | Problem |
|---|---|---|
| Tailwind | `build` always runs the **v4** CLI (`@tailwindcss/cli`); `dev` loads the v4 browser CDN | Every official template uses **v3** syntax (`@tailwind base;` + `tailwind.config.js` with the typography plugin). Building the blog template produces a **2 KB** CSS file with no utilities, and `prose` never works in dev |
| Asset build | `exec()` for Tailwind/esbuild is fire-and-forget | "Successfully built" prints before CSS/JS exist; failures don't fail the build |
| Conditions | `<If condition>` is evaluated with `new Function(...)` | Arbitrary JS execution from templates and front matter |
| Parsing | Non-greedy regexes for `<ForEach>`, `<If>`, attributes | Nested loops break; `condition="a > 1"` breaks (the `>` ends the tag); `<If>` outside a loop is never processed and leaks into HTML |
| Includes | Only one level; only inside a `<layout>` | Includes in includes don't work; pages without a layout keep raw `<include>` tags |
| Content loops | `<ForEach content>` writes `collections/content/*.json` **into your source tree** on every render | Dirty git status; races in dev |
| Content listing | `readdirSync` on the folder | A sub-folder inside e.g. `content/posts` crashes the build |
| Front matter | `JSON.parse('…')` with only `'` escaped | Backslashes or `</script>` in front matter break or inject into the page |
| Official blog template | `includes.md` mentions `<include src="message.html">` in prose | **The original crashes building its own blog template** (ENOENT, stack trace) |
| Build dir | `build.directory` from static.json is deleted recursively | Setting it to `.` or a source folder deletes your project |
| `new` | Hard-coded `/archive/main.zip`, always runs `npm install && static build && dev` | Breaks for `master` repos; can't use your own template |
| 404 | `pages/404.html` builds to `404/index.html` | Static hosts (GitHub Pages, Netlify, Cloudflare) expect `/404.html` |
| Live reload | Separate livereload server on port 35729 + `connect-livereload` | Two ports; reload fires on build output too |
| Version | `static --version` reads the *global* npm install path | Wrong/crashes with npx or local installs |

## 3. Architecture

```
src/
  main.rs            CLI (clap): new, dev, build, serve, templates
  site.rs            project model: routing, data sources, page/content rendering
  template/
    tags.rs          quote-aware scanner for <layout> <include> <page> <ForEach> <If> <ElseIf> <Else>
    compose.rs       string-level composition: <page>, nested <layout>s, recursive <include>s
    mod.rs           AST for ForEach/If, scoped renderer, placeholder substitution
    expr.rs          safe expression evaluator for conditions
  markdown.rs        front matter (YAML), pulldown-cmark, GitHub heading ids, [toc]
  assets.rs          copy public/ + assets/, Tailwind v3/v4 detection, esbuild, tool discovery
  build.rs           parallel build (rayon), sitemap, safety checks
  dev.rs             axum dev server, SSE live reload, file watcher, error overlay, `serve`
  new.rs             template download/extract (zip-slip safe), local templates
  config.rs          static.json, .env
```

Rendering pipeline for one page:

```
page.html ──compose──▶ HTML with {placeholders}, <ForEach>, <If>
          (page → layout(s) → includes, layout attrs → {title})
       ──shortcodes──▶ {tailwindcss}
       ──parse──▶ AST (balanced ForEach/If, errors with file:line)
       ──render(scope stack)──▶ HTML
       ──urls──▶ { url('/x') } resolved
```

Placeholders resolve against a **scope stack** (`frontmatter`, `content`,
`toc`, `page`, `site`, `env`, loop variables, `loop`, `$loop`). Unknown
placeholders are left untouched, so inline JS such as `x-data="{open}"` is safe.
Markdown output is inserted as a value and never re-templated.

## 4. Improvements (all implemented)

Compatibility fixes: Tailwind v3 vs v4 auto-detected for both dev and build;
waits for Tailwind/esbuild and fails loudly; safe conditions; real nesting;
no writes to your source tree; `404.html` at the root; no crash on the official
blog template; build-dir safety check; default-branch-agnostic `new`.

New features (opt-in, backwards compatible):

- `<ElseIf condition>` / `<Else>`
- Nested `<ForEach>` over an item field: `<ForEach collection="post.tags" as="tag">`
- `<ForEach>` attributes `where="…"`, `offset="…"` alongside `count`, `orderBy`, `as`
- `{$loop.index}`, `{$loop.first}`, `{$loop.last}`, `{$loop.count}`
- Conditions: `&&`/`||`/`!`/`and`/`or`/`not`, `contains`, `.length`, parentheses
- Nested layouts (a layout may use `<layout src>` itself)
- Includes anywhere (pages without layouts, markdown, other includes) with cycle detection; `<include src="x" />`
- `{site.*}` from `static.json`, `{env.*}` from `.env` only (never the shell environment), `{page.url}` for active links
- `post.html` (rendered markdown) and `post.slug` on content items
- `draft: true` front matter: visible in `dev`, excluded from `build` and from loops
- `sitemap.xml` when built with an absolute URL
- `static serve` to preview the production build; `static templates`
- `static new site --template owner/repo | ./local-folder | file.zip`
- Dev server: one port, SSE live reload, in-browser error overlay, canonical URLs, path-traversal protection, esbuild cleaned up on exit
- Build reports *every* failing page with `file:line`, renders pages in parallel

## 5. Benchmarks

Blog template + 1,000 generated posts, pages only (Tailwind/esbuild excluded
for both), 2 vCPU Linux:

| | time |
|---|---|
| `@devdojo/static` 1.1.0 (Node 22) | 1,303 ms |
| `static` (Rust, release) | **112 ms** |

Rendered HTML is identical apart from intentional fixes (safe `window.frontmatter`
literal, code-block class names).

## 6. Testing

- Unit tests per module (scanner, expressions, composition, markdown, routing, zip extraction).
- `tests/kitchen_sink.rs` builds `tests/fixtures/kitchen-sink`, which exercises every feature.
- Manually verified against the official `starter` and `blog` templates, diffing
  output with the original.

## 7. Roadmap (not done yet)

- Syntax highlighting for fenced code (e.g. `syntect`), opt-in.
- Incremental builds (dependency graph from compose step) — only re-render pages affected by a change.
- Pagination for content collections (`<ForEach … paginate="10">` → `/blog/page/2/`).
- RSS/Atom feed from a content folder.
- Bundle a pinned Tailwind standalone binary on first use, to remove the Node dependency entirely.
- CSS-only hot swap in dev (no full reload for stylesheet changes).
- Prebuilt binaries via the release workflow + `npm` shim package so `npx @devdojo/static` keeps working.

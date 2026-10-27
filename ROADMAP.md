# Statico roadmap

Priorities come from [docs/RESEARCH.md](docs/RESEARCH.md): what people who use
Hugo, Zola, Eleventy, Astro and Jekyll say they miss. Statico's bet is the
combination: **plain HTML, one binary, batteries included, and
built to be driven by agents.**

## Done in 0.1

**Core (Static-compatible)** — pages, layouts (nested), includes (recursive,
cycle-safe, in markdown too), `<ForEach>` (nested, `where`, `offset`,
`orderBy`, `count`), `<If>/<ElseIf>/<Else>` with a safe expression language,
markdown content with front matter, drafts, `[toc]`, `static.json` support.

**Content features** — pagination (`paginate="10"` → `/page/2/`), taxonomy
pages (`[tag].html`), Atom feeds, syntax highlighting, mermaid blocks,
placeholder filters (`date`, `truncate`, `default`, `slug`, …), YAML/TOML/CSV
collections, front matter schemas, `{seo}` (meta, Open Graph, Twitter, JSON-LD).

**Build** — parallel rendering with shared, single-flight caches; HTML
minification; fingerprinted CSS/JS; image `width`/`height` + lazy loading;
sitemap; `search.json` or Pagefind; Tailwind v3/v4 auto-detection with a
managed standalone binary (no Node); managed esbuild; safety checks on the
output folder; `--watch`.

**Agent-ready CLI** — `--json` on every command with stable error codes,
`statico check`, `statico add`, `statico routes`, `statico docs`, `statico explain`,
`statico diff`, `statico schema`, `statico agents` (AGENTS.md + CLAUDE.md),
`llms.txt`, `llms-full.txt`, `.md` mirrors, `Accept: text/markdown` in dev.

**Distribution** — release workflow for 5 targets, `curl | sh` installer,
npm wrapper, Homebrew formula template (fill in the repository name).

## Next (in priority order)

1. **Image pipeline** — resize to several widths, WebP/AVIF, `srcset`/`sizes`
   on plain `<img>` tags; cache results by content hash. *(#5 in research)*
2. **Components with slots** — `<include src="card.html">inner html</include>`
   exposed as `{slot}`, usable from markdown. *(#9)*
3. **On-disk build cache** — hash every input and skip unchanged pages, so
   100k-page sites rebuild in seconds. Today a full 1,000-page build takes about
   a second, so this matters only at scale. *(#10)*
4. **OG image generation** — a small HTML/CSS template rendered to PNG per page
   (resvg), no headless browser. *(#12)*
5. **`statico cms init`** — generate Decap/Sveltia/Pages CMS config from content
   folders and schemas, so non-developers can edit. *(#6)*
6. **Importers** — `statico import jekyll|hugo|wordpress` for content and front matter.
7. **i18n** — `i18n/<lang>.toml` strings with `{name}` placeholders and
   language-prefixed routes. *(#14)*
8. **`statico dev --json`** — NDJSON events (rebuilds, errors) for agents watching the server.
9. **Checksums** for managed tool downloads, and Subresource Integrity hashes for assets.
10. **Optional `statico mcp`** — a thin wrapper over the JSON commands for chat
    clients that have no terminal. Deliberately last: agents with a shell are
    better served by the CLI.

## Stability policy (proposed)

- Template syntax, folder conventions and `statico.json` keys are stable within a
  major version; deprecations warn for at least one minor release with a code
  and a fix hint before anything is removed.
- Every release is checked against the fixture sites with `statico diff`; a
  change in output is called out in the changelog.
- Users can do the same: build with the old and new version into two folders
  and run `statico diff old new`.

## Non-goals

- A JavaScript framework, hydration or client-side routing.
- A plugin API in v0.x — features ship in the binary so every site keeps
  working on upgrade.

## Appendix: audit of the original Static (Node.js) 1.1.0

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


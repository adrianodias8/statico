# What people want from a static site generator (research, Oct 2026)

Desk research across Hacker News, Lemmy/Reddit roundups, GitHub discussions,
project forums, vendor blogs and the State of JS 2025 survey. About 50 searches
and fetches. Reddit, the GitHub search API, llmstxt.org and some HN pages were
not reachable directly; their content is cited through pages that discuss them.

## Summary

- **Stability is the #1 trust issue.** Breaking upgrades (often silent) are the
  most consistent complaint about Hugo, Eleventy v3, Astro v6 and Gatsby.
- **Ownership churn has users looking for something independent.** Gatsby is in
  maintenance mode; Eleventy moved to Font Awesome and was renamed "Build
  Awesome"; Cloudflare acquired Astro in January 2026.
- **Learning a template language is the top barrier.** People repeatedly ask for
  "just HTML with includes" — Statico's core idea.
- **Single binary, no Node** is a headline feature (Zola, Hugo). Hugo 0.161
  requires Node 22+ for its Tailwind/PostCSS pipeline, which leaves room for a
  Tailwind-ready tool that doesn't need Node.
- **Biggest missing features:** built-in image handling and editing for
  non-developers; expected baseline: search, OG images, link checking, extra
  output formats.
- **AI era:** ship `.md` copies of pages and `llms.txt` as cheap agent features,
  not as SEO — crawlers rarely fetch `llms.txt`, but coding agents and docs tools
  use it, and agents send `Accept: text/markdown`.
- **CLI with `--json` before MCP.** For local tools the 2026 consensus is CLI
  first; MCP is for authenticated, multi-system work.
- **Name:** `statico` is taken on both crates.io and npm, so the package ships
  as `statico-cli` on both; the binary is `statico`. Homebrew was free.

## Top pain points (ranked) and what Statico does about them

| # | Pain point | Evidence (sources below) | Statico status |
|---|---|---|---|
| 1 | Breaking upgrades | Julia Evans' Hugo 0.40→0.135 upgrade (~80/700 posts edited, found by diffing HTML); Hugo 0.158–0.164 breaking changes; HN on Hugo renames; Astro v6 guide | ✅ `statico diff old new` compares builds; stability policy in ROADMAP |
| 2 | Template-language learning curve | Lemmy "SSG for an idiot…" thread; Ask HN "simplest SSG" | ✅ HTML + 5 tags; filters are optional |
| 3 | Node dependency sprawl | Smashing's Gatsby retrospective (168 deps); 11ty v3 ESM pain | ✅ single binary |
| 4 | Tailwind without Node | Hugo 0.161 needs Node 22+; Tailwind's standalone CLI exists for this | ✅ downloads + caches the official standalone CLI; v3/v4 auto-detect |
| 5 | Image optimisation | Zola/Hugo need hand-written srcset macros; Next export can't optimise | 🟡 width/height + lazy loading; resizing/WebP/AVIF next |
| 6 | Editing for non-developers | HN: marketing teams keep WordPress; Decap's dated UI; Tina needs React + SaaS | ⏭ `statico cms init` (Decap/Sveltia/Pages CMS config) planned |
| 7 | Errors that don't name the file | Hugo forum nil-pointer with no file; Astro's errors as the model | ✅ file:line, codes, `statico explain`, JSON |
| 8 | Front matter validation | Astro content schemas | ✅ `schemas` in statico.json (S201) |
| 9 | Components that can't nest / contain markdown | Zola shortcodes → user went back to Hugo | 🟡 includes nest and work in markdown; slots planned |
| 10 | Performance at scale, incremental builds | Hugo 100k+ page threads; Gatsby Cloud's incremental builds gone | 🟡 parallel builds, shared caches; on-disk incremental cache planned |
| 11 | Search | Pagefind is the default; Tim Bray's notes on its rough edges | ✅ `search.json` or `"pagefind"` |
| 12 | OG/social images | Hugo users resort to screenshot services; Astro uses Satori | ⏭ planned (HTML subset → PNG) |
| 13 | Custom output formats | Hugo's output formats make llms.txt easy | ✅ feeds, llms.txt, `.md` mirrors, sitemap, search.json |
| 14 | i18n with placeholders | Zola lacks context variables in translations | ⏭ planned |
| 15 | Link checking, accessibility, QA | Separate add-ons today (Astro link validator, jekyll-audit) | ✅ `statico check` (links, anchors, alt, title, lang, placeholders) |

Also mentioned: importers from WordPress/Jekyll/Hugo, Subresource Integrity, ActivityPub.

## Competitors

| Tool | Language | Install | Strengths | Top complaints |
|---|---|---|---|---|
| Hugo | Go | Single binary (Node 22+ for Tailwind/PostCSS) | Speed, image pipeline, output formats | Go templates, breaking changes, unclear errors, memory at 100k+ pages |
| Zola | Rust | Single binary | Simple, Tera templates | No plugins, limited shortcodes, no asset pipeline, weak i18n |
| Eleventy / Build Awesome | JS | npm | Flexible, HTML-friendly | v3 ESM migration, JS config, ownership/rename uncertainty |
| Astro | JS/TS | npm | Top State of JS satisfaction, schemas, errors | Complexity, v6 breaking changes, now Cloudflare-owned |
| Jekyll | Ruby | gem | GitHub Pages, stable | Slow, Ruby toolchain |
| Gatsby | JS | npm | Data layer, images | Maintenance mode, dependency weight |
| DevDojo Static | JS | npm | Page routing, `<include>`, ForEach, Tailwind | Small project, Node-based (Statico's starting point) |

## AI / agent readiness

- **llms.txt** (proposed Sep 2024): found on ~28% of 137k domains (Ahrefs, June
  2026), yet ~97% of valid files received no requests in May 2026; Google says
  it isn't needed for search. Cheap to generate, useful for docs/coding agents.
- **Markdown copies:** Cloudflare's "Markdown for Agents" (Feb 2026) serves
  markdown for `Accept: text/markdown` and reports ~80% fewer tokens; Claude Code
  and OpenCode reportedly send that header.
- **AGENTS.md** is a founding project of the Linux Foundation's Agentic AI
  Foundation (Dec 2025). Claude Code reads CLAUDE.md, hence Statico writes both.
- **How agents build sites today:** conventions files, existing components and
  reliable dev-server feedback matter most; separate accessibility audit passes help.
- **CLI vs MCP:** CircleCI (Mar 2026) and Kaxil's benchmarks favour CLIs for
  local work (fewer tokens, higher task scores); MCP fits authenticated,
  multi-system tools. Statico ships `--json` everywhere; an optional `statico mcp`
  wrapper can come later for chat clients without a terminal.

## Market signals

State of JS 2025: Astro has the highest meta-framework satisfaction; Next.js
leads usage with falling satisfaction; top pain points across frameworks are
cognitive overload, breaking changes, too much magic and bloat. Popularity has
come from speed + single binary (Hugo, Zola), zero-JS-by-default + docs +
errors (Astro), and simplicity (11ty).

## Name check: "statico"

Checked Oct 2026 (the project was renamed from "Forno" to "Statico"):

- crates.io: `statico` is taken by an unrelated crate — published as
  `statico-cli`; the binary and library are still `statico`.
- npm: unscoped `statico` is taken by a 2016 package — the wrapper is published
  as `statico-cli`, installing a `statico` binary.
- Homebrew: free. PyPI and domains were not checked.
- Similar names, no direct conflict: Statik, Staticrypt.
- Trademark databases were not checked.

## Sources

- https://jvns.ca/blog/2024/10/07/some-notes-on-upgrading-hugo/
- https://www.docsy.dev/blog/2026/hugo-0.158.0+/index.md
- https://news.ycombinator.com/item?id=47735535
- https://brennan.day/the-end-of-eleventy/
- https://www.11ty.dev/blog/build-awesome/
- https://www.infoq.com/news/2026/02/astro-v6-beta-cloudflare
- https://itbrief.co.uk/story/cloudflare-buys-astro-framework-pledges-open-future
- https://github.com/gatsbyjs/gatsby/discussions/39062
- https://smashingmagazine.com/2024/03/end-of-gatsby-journey
- https://dev.to/vst/why-i-am-migrating-from-zola-back-to-hugo-1k54
- https://zola.discourse.group/t/using-hugo-but-wanna-be-zola/2379
- https://dev.to/robole/eleventy-is-it-time-to-upgrade-to-version-3-4c72
- https://deddit.petersanchez.com/g/selfhosted@lemmy.world/p/K83LK15K4ChbkcTSY2-Static-site-generator-for-an-idiot-who-doesnt-wa
- https://hn.nuxtjs.org/item/39861593
- https://gummysearch.com/tools/best-products/static-site-generator/
- https://discourse.gohugo.io/t/how-do-you-debug-incorrect-content-pages-the-debug-logs-dont-show-what-went-wrong/50971
- https://discourse.gohugo.io/t/hugo-sites-over-100-000-pages-issues-remedies/55369/2
- https://discourse.gohugo.io/t/render-html-to-a-png-file-at-build-time/54097
- https://talk.jekyllrb.com/t/slow-jekyll-build-time-how-to-improve/9981
- https://constantin.glez.de/notes/2025-06-01-adding-responsive-and-efficient-images-to-your-blog/
- https://nextjs.org/docs/messages/export-image-api
- https://tailwindcss.com/blog/standalone-cli
- https://docs.rs/encre-css
- https://www.tbray.org/ongoing/When/202x/2025/11/01/Blog-Search-Pagefind
- https://lib.rs/crates/pagefind
- https://www.luckymedia.dev/compare/decap-cms-vs-tina-cms
- https://docs.astro.build/en/reference/errors/markdown-content-schema-validation-error
- https://dev.to/morellodev/generating-og-images-at-build-time-with-satori-and-astro-4b4b
- https://travis.media/blog/astro-broken-links-validator/
- https://dev.to/brennan/shipping-jekyll-audit-dogfooding-a-jekyll-qa-cli-on-a-production-site-307j
- https://2025.stateofjs.com/en-US/libraries/meta-frameworks/
- https://strapi.io/blog/state-of-javascript-2025-key-takeaways
- https://www.11ty.dev/blog/jamstack-survey-2022
- https://jamstack.org/generators/
- https://www.ayautomate.com/blog/llms-txt
- https://www.wix.com/studio/ai-search-lab/llms-txt-myths
- https://ravichaganti.com/blog/implementing-llms.txt-and-markdown-output-in-hugo/
- https://thenewstack.io/cloudflares-markdown-for-agents-automatically-make-websites-more-aifriendly
- https://codex.danielvaughan.com/2026/04/07/agents-md-open-standard-cross-tool-portability
- https://circleci.com/blog/mcp-vs-cli
- https://kaxil.substack.com/p/mcp-vs-cli-vs-rest
- https://leonfurze.com/2026/02/14/building-websites-with-claude-code/
- https://dev.to/aimiten/i-rebuilt-our-company-website-with-astro-and-claude-code-heres-what-actually-worked-4c3o
- https://github.com/thedevdojo/static
- https://gohugo.io/tools/migrations/
- https://crates.io/api/v1/crates/statico

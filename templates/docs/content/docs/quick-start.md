---
title: Quick start
description: From an empty folder to a published documentation site in four commands.
section: start
order: 2
---

## Create the site

```bash
statico new my-docs --template docs
cd my-docs
statico dev
```

Statico is a single binary: there is no Node.js or npm step. `statico dev` serves the site at `http://localhost:3000` and reloads the page when you save a file.

## Make it yours

1. Set the project name, description and repository link in `statico.json`.
2. Rename the sections in `collections/sections.json`.
3. Replace the pages in `content/docs/` with your own.

<include src="note.html" kind="tip" text="Keep the description in each page's front matter to one line. It is shown under the page title, in search results and in link previews." />

## Check, then build

```bash
statico check
statico build https://docs.example.com
```

`statico check` builds the site into a temporary folder and reports broken links, missing anchors, images without alt text and front matter that does not match the schema. When it reports zero errors, `statico build` writes the site to `_site/`.

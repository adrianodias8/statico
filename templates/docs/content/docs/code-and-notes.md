---
title: Code and notes
description: Highlighted code blocks, inline code, tables, and notes that stand out.
section: write
order: 4
---

## Code blocks

Name the language after the opening fence and the block is highlighted when the site is built:

```toml
[server]
port = 3000
open = true
```

```bash
statico build https://docs.example.com --out public
```

Inline code, like `statico check --strict`, is set in the same monospace face.

## Notes

Use the `note.html` include for anything a reader must not miss. It works inside markdown:

```html
<include src="note.html" kind="warning" text="This deletes the output folder before every build." />
```

<include src="note.html" kind="note" text="A note adds information the reader may want." />

<include src="note.html" kind="warning" text="A warning is for something that can lose work or break a site." />

## Tables

| Command | What it does |
|---|---|
| `statico dev` | Serves the site and reloads on save |
| `statico check` | Builds into a temporary folder and verifies it |
| `statico build` | Writes the production site to `_site/` |

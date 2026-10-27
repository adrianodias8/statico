---
title: Introduction
description: What Statico is, what it is for, and why it stays plain HTML.
section: start
order: 1
---

Statico turns plain HTML into static sites. Pages are HTML files, layouts and includes are HTML files, and five tags do the rest: `<layout>`, `<include>`, `<ForEach>`, `<If>` and `<page>`. There is no framework to learn and no template language beyond those tags.

It is a single binary, written in Rust. There is no Node.js, no npm and no build configuration to write; Tailwind and esbuild are fetched on demand, checked against pinned checksums, and only when a site actually uses them.

## Why Statico

- **Plain HTML.** If you can write a web page, you can build a site.
- **Batteries included.** Markdown content, collections from JSON, YAML, TOML or CSV, pagination, tag pages, Atom feeds, sitemaps, search, SEO tags and syntax highlighting, all built in.
- **Fast.** Pages render in parallel; a site of a thousand pages builds in about a second.
- **Checked.** `statico check` finds broken links, missing anchors, images without alt text, typos in placeholders and front matter that doesn't match its schema, with stable error codes you can look up.
- **Built to be driven by agents.** Every command has `--json`, errors carry a code, a file and a line, and `statico agents` writes the conventions down for AI coding tools.

## Where it came from

Statico began as a Rust rewrite of [DevDojo's Static](https://github.com/thedevdojo/static) and still builds Static projects unchanged: `static.json` works as it is, and so do the official templates.

This documentation is itself a Statico site, built from the `docs` template that ships in the binary.

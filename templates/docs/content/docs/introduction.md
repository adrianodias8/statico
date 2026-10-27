---
title: Introduction
description: What this template gives you, and how a page becomes part of the book.
section: start
order: 1
---

This is a documentation site built with [Statico](https://github.com/adrianodias8/statico). Every page you read here is one markdown file, and the contents, the sidebar, the outline on the right and the links to the previous and next page are all worked out from those files. There is nothing else to keep in sync.

These first pages document the template itself, so they double as a working example. Replace them with your own as you go.

## What you get

- A sidebar grouped into sections, in the order you choose.
- An outline of the current page, built from its headings.
- Search across every page, with no service to sign up for.
- Previous and next links, so the docs read front to back like a book.
- Code blocks highlighted at build time, and notes for the things readers must not miss.

## How a page joins the book

Each page declares where it belongs in its front matter:

```yaml
---
title: Introduction
description: One line about what the page answers.
section: start
order: 1
---
```

`section` names one of the sections in `collections/sections.json`, and `order` places the page in the book. That is the whole system.

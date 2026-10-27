---
title: Pages and the sidebar
description: Adding a page, ordering pages, and creating new sections.
section: write
order: 3
---

## Adding a page

Create a markdown file in `content/docs/`. Its file name becomes its address: `pages-and-the-sidebar.md` is published at `/docs/pages-and-the-sidebar/`.

Every page needs four fields, and the build stops with error `S201` if one is missing:

| Field | What it does |
|---|---|
| `title` | The page heading, its entry in the sidebar and in search |
| `description` | The line under the heading and in search results |
| `section` | Which group of the sidebar it appears in |
| `order` | Its position in the book, used by the sidebar and by previous and next |

## Ordering

Pages are ordered by `order` across the whole book, not only within their section, so previous and next always run straight through. Leave gaps (10, 20, 30) if you expect to insert pages later.

## Sections

Sections are listed in `collections/sections.json`, in the order they appear:

```json
{ "name": "write", "title": "Writing pages", "hue": "teal" }
```

`name` is what pages put in their `section` field; `title` is what readers see. `hue` picks the colour of the section's hairline: `rose`, `teal`, `amber`, `indigo` or `olive`.

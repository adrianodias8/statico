---
title: Welcome aboard
description: What this template is, where everything lives, and the three files you will edit most.
date: 2026-10-20
tags: [start-here]
---

This blog is a [Statico](https://github.com/adrianodias8/statico) site. Everything you see is built from plain files, and the build is a single command with nothing to install alongside it.

The home page is a timetable. Every post is a departure: its date, its title, the lines it runs on, and its journey time, which is how long it takes to read. The timetable is the archive, so there is nothing else to maintain.

## The three files you will edit most

1. **`content/posts/`** holds one markdown file per post. The file name becomes the address: `welcome-aboard.md` is published at `/posts/welcome-aboard/`.
2. **`collections/lines.json`** lists the lines (tags), each with a one-letter code and a colour.
3. **`statico.json`** has the blog's name, description and author.

## Running it

```bash
statico dev       # preview at http://localhost:3000, reloads as you save
statico check     # broken links, missing alt text, typos in placeholders
statico build https://your-domain.example
```

When `statico check` reports zero errors, the blog is ready to publish. The [next post](/posts/writing-a-post/) covers writing one.

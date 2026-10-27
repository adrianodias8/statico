---
title: Writing a post
description: Front matter, descriptions that earn the click, drafts, and how lines are assigned.
date: 2026-10-21
tags: [start-here, writing]
---

A post is a markdown file with a few lines of front matter at the top, between two `---` lines:

```yaml
---
title: Writing a post
description: One sentence that tells a reader why to open this.
date: 2026-10-21
tags: [writing]
---
```

Every field matters somewhere. The **title** heads the post and its row in the timetable. The **description** appears under the title in the table, in search results and in link previews, so write it for someone deciding whether to read on. The **date** orders the timetable and the feed. The **tags** choose the post's lines.

## Drafts

Add `draft: true` and the post shows up while you run `statico dev` but is left out of `statico build`. Remove the line when it is ready.

## Checking your work

`statico.json` declares what every post must have:

```json
"schemas": { "posts": { "title": "string", "date": "date", "description": "string", "tags": "list" } }
```

Forget a description and the build stops with error `S201`, naming the file and the missing field, instead of publishing a post with an empty row in the timetable.

## Lines

A line is a tag with a code and a colour. Use any of the names in `collections/lines.json`, or add a new line there:

```json
{ "name": "travel", "title": "Travel", "code": "T", "color": "blue" }
```

The colours available are `red`, `blue` and `green`; adding more is three lines of CSS in `assets/css/main.css`.

---
title: Lines, the feed and longer timetables
description: How tag pages are made, how readers subscribe, and what happens when the timetable gets long.
date: 2026-10-26
tags: [reference]
---

## Lines

Each line gets its own page, listing only its departures: [Reference](/lines/reference/) is one. Statico makes these pages from `pages/lines/[tag].html`, one for every tag used in a post. A line you define but never use simply has no page.

## The feed

Readers can follow the blog in any feed reader at `/posts/feed.xml`. The feed is written when you build with your site's address:

```bash
statico build https://your-domain.example
```

Without the address, feed links would be relative, which most feed readers cannot follow, so Statico warns you.

## Longer timetables

The home page shows twenty departures per sheet. When there are more, the build writes `/page/2/`, `/page/3/` and so on, with links between them at the foot of the table. Change the number in `pages/index.html`:

```html
<ForEach content="posts" as="post" orderBy="date, desc" paginate="20">
```

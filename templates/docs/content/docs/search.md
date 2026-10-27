---
title: Search
description: How search works, and what to do if it shows nothing.
section: ship
order: 5
---

The search box at the top searches every page's title, description and text. It needs no external service: `statico build` writes a `search.json` file next to your pages, and a small script in `assets/js/search.js` reads it in the reader's browser.

## Turning it on and off

Search is enabled in `statico.json`:

```json
{ "build": { "search": true } }
```

Set it to `"pagefind"` instead to use [Pagefind](https://pagefind.app) for very large sites.

## Using it

Type to see the best matches as you go; use the arrow keys and Enter to open one. Press Enter without choosing to see every match on the search page.

<include src="note.html" kind="note" text="search.json is written by statico build, so while you run statico dev the box asks you to build first." />

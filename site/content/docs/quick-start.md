---
title: Quick start
description: Create a site from a template, preview it, check it, and build it.
section: start
order: 3
---

## Create a site

```bash
statico new my-site --template blog
cd my-site
statico dev
```

`statico dev` serves the site at `http://localhost:3000` and reloads the page when you save. Three templates ship inside the binary and work offline: `portfolio`, `blog` and `docs`. Any GitHub repository, local folder or zip file works too; `statico templates` lists them.

To turn an existing folder into a site instead, run `statico init` inside it.

## Add pages

```bash
statico add page about --title "About"
statico add post blog "My first post"
```

`statico add` creates files in the right place with the right boilerplate. A page is an HTML file in `pages/`, and its path is its address: `pages/about.html` is published at `/about/`.

## Check, then build

```bash
statico check
statico build https://my-site.example
```

`statico check` builds into a temporary folder and verifies the result: zero errors means the site is ready. `statico build` writes it to `_site/`, ready for any static host. See [Deploying](/docs/deploying/) for GitHub Pages and others.

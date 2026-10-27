---
title: Deploying
description: Publishing to GitHub Pages, including project sites under a sub-path, and to any other static host.
section: ship
order: 22
---

`statico build` writes a complete static site to `_site/`. Any host that serves files can publish it. Always pass the site's address: canonical links, feeds, the sitemap and search use it.

## GitHub Pages

This workflow checks the site, builds it and deploys it on every push to `main`:

```yaml
name: Deploy
on:
  push:
    branches: [main]
  workflow_dispatch:
permissions:
  contents: read
  pages: write
  id-token: write
concurrency:
  group: pages
  cancel-in-progress: false
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
    steps:
      - uses: actions/checkout@v4
      - name: Install Statico
        run: curl -fsSL https://raw.githubusercontent.com/adrianodias8/statico/main/packaging/install.sh | sh
      - run: ~/.local/bin/statico check
      - run: ~/.local/bin/statico build https://you.github.io/your-repo
      - uses: actions/upload-pages-artifact@v3
        with:
          path: _site
      - uses: actions/deploy-pages@v4
```

In the repository's settings, under **Pages**, set the source to **GitHub Actions**.

## Project sites and sub-paths

A repository's GitHub Pages site lives under a sub-path: `https://you.github.io/your-repo/`. Build with that full address and Statico moves every root-relative URL inside it, in pages and stylesheets alike: `href="/docs/"` is published as `href="/your-repo/docs/"`, and a font at `url(/assets/fonts/…)` keeps loading. You write links from the root as usual.

This site is one: it is published at `https://adrianodias8.github.io/statico/`.

## Other hosts

Netlify, Cloudflare Pages, Vercel and similar hosts need two settings: the build command `statico build https://your-domain`, and the output folder `_site`. `pages/404.html` is published as `/404.html`, which is where these hosts look for a not-found page.

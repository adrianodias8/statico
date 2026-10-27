---
title: Deploying
description: Publishing the built site to GitHub Pages or any static host.
section: ship
order: 6
---

`statico build` writes a complete static site to `_site/`. Any host that serves files can publish it.

## GitHub Pages

A workflow that builds and deploys on every push to `main`:

```yaml
name: Deploy docs
on:
  push:
    branches: [main]
permissions:
  contents: read
  pages: write
  id-token: write
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: github-pages
    steps:
      - uses: actions/checkout@v4
      - name: Install Statico
        run: curl -fsSL https://raw.githubusercontent.com/adrianodias8/statico/main/packaging/install.sh | sh
      - run: ~/.local/bin/statico check
      - run: ~/.local/bin/statico build https://you.github.io/your-project
      - uses: actions/upload-pages-artifact@v3
        with: { path: _site }
      - uses: actions/deploy-pages@v4
```

## Other hosts

Netlify, Cloudflare Pages and similar hosts need two settings: the build command `statico build https://your-domain` and the output folder `_site`.

<include src="note.html" kind="tip" text="Always pass your site's address to statico build. Search, canonical links and the sitemap use it." />

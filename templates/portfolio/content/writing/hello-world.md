---
title: Hello World 🌍
description: My first post, on why I went with a static site and how this blog is built and deployed.
date: 2025-04-08
---

![Hello World](/assets/images/posts/hello-world.jpeg)

> **Updated, October 2026** — this post originally walked through building the blog with Static by DevDojo. The site now runs on [Statico](https://github.com/adrianodias8/statico), a static site generator I ended up writing in Rust. The commands and the deploy workflow below have been updated to match. Everything else is as it was.

Oh, hi there! 👋 Welcome to my very first blog post. Think of it like that awkward "Hello, World!" program we all wrote when first learning to code.

I've decided to kick things off by embracing the concept of less is more, mostly because I'm lazy—oops, I meant "efficient" and I found out static sites are an awesome way to achieve this. Plus, who has time for complicated CMS setups, database migrations, updates, etc etc...

## 🚀 Static Websites: When Less Really is More

You might be thinking: "Static websites? Sounds boring!" But hear me out, they're like the IKEA furniture of web development—sleek, functional, and occasionally frustratingly simple. No moving parts means fewer things to break, less to maintain.

I started out with Static by DevDojo because:

- It's free (my favorite price).
- It's easy (my favorite difficulty level).
- You can get something looking nice really fast.

That's what got this blog off the ground, and the idea behind it is still exactly what I wanted: pages are just HTML files, with a handful of tags for layouts, includes and loops. No template language to learn.

What I kept tripping over was everything around it. It needs Node, the Tailwind setup broke in ways that were hard to pin down, and when something went wrong the error rarely told me which file to look at. So I did the reasonable, proportionate thing and rewrote it in Rust. 🙃

That's **Statico**: same HTML, same folders, same tags — one binary instead of a Node toolchain. My site files didn't change at all when I switched, which is the part I'm most pleased about.

## 🛠️ How to Statico Like a Pro (or at least Fake It)

Here's the quick guide to getting started:

Install Statico: it's a single binary, so there's nothing else to set up. Open your terminal and run:

`curl -fsSL https://raw.githubusercontent.com/adrianodias8/statico/main/packaging/install.sh | sh`

This drops a `statico` binary into `~/.local/bin`. Builds for other platforms are on the [releases page](https://github.com/adrianodias8/statico/releases).

Create a New Project: Navigate to the directory where you want your project to reside and run:

`statico new my-static-site`

Replace my-static-site with your desired project name. This command creates a new folder with all the necessary files for your static site.

Navigate to Your Project Folder:

`cd my-static-site`

Move into your project's directory to start working on it.

Start the Development Server: Launch the local development server with:

`statico dev`

Your site is now running locally at http://localhost:3000/, and it reloads by itself when you save a file. Open this URL in your browser to view your site.

Check Your Work: Before you push anything, run:

`statico check`

This builds the site and tells you about broken links, missing `#anchor` targets, images without alt text and placeholders you typo'd. Zero errors means you're good.

Build for Production: Once you're satisfied with your site, generate the production-ready files by running:

`statico build`

This command creates a `_site` folder containing your optimized static files, ready for deployment.

Congratulations, you're now the proud owner of a static website!

## 🤔 Why Static Websites Are Actually Awesome

⚡ Speed: Super fast loading times due to optimized static assets.

🔐 Security: Less vulnerable to hacking because, honestly, there's nothing dynamic to hack.

💰 Cost-effective: Hosting is virtually free. For example, GitHub Pages offers free hosting.

## 🌐 Deploying Your Static Website on GitHub Pages

Let's take advantage of GitHub Pages to host your static site completely for free! Follow these simple steps to deploy your site automatically each time you update your repository.

Step-by-Step Guide for GitHub Pages

1. Create a GitHub Repository:

Visit GitHub and create a new repository (for example, my-static-site).

Clone it locally, or set your existing project to track this remote.

```
git init
git add .
git commit -m "Initial commit"
git branch -M main
git remote add origin https://github.com/yourusername/my-static-site.git
git push -u origin main 
```

Replace yourusername and my-static-site with your GitHub username and repo name.

2. Add GitHub Actions Workflow:

In your repository, you should find new folder `.github/workflows` and a `static.yml`.

***Note on GitHub Workflow***

Back when this ran on Static, the default workflow didn't work for me — some of the actions it used were outdated, and I added a [pull request](https://github.com/static-templates/aria/pull/4) so hopefully someone from DevDojo updates it in their templates.

The current one is shorter, because there's no Node to install. It downloads one binary, checks the site, builds it and deploys:

`static.yml`
```
# Simple workflow for deploying static content to GitHub Pages
name: Deploy static content to Pages

on:
  # Runs on pushes targeting the default branch
  push:
    branches: ["main"]

  # Allows you to run this workflow manually from the Actions tab
  workflow_dispatch:

# Sets permissions of the GITHUB_TOKEN to allow deployment to GitHub Pages
permissions:
  contents: read
  pages: write
  id-token: write

# Allow only one concurrent deployment, skipping runs queued between the run in-progress and latest queued.
# However, do NOT cancel in-progress runs as we want to allow these production deployments to complete.
concurrency:
  group: "pages"
  cancel-in-progress: false

env:
  # Pin the generator so a build is reproducible; bump to upgrade.
  STATICO_VERSION: v0.1.0
  SITE_URL: https://adwebdev.xyz

jobs:
  # Single deploy job since we're just deploying
  deploy:
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Pages
        uses: actions/configure-pages@v4

      # Statico fetches standalone Tailwind and esbuild binaries on first use.
      # Caching them keeps every run after the first one offline and fast.
      - name: Cache Statico toolchain
        uses: actions/cache@v4
        with:
          path: ~/.cache/statico
          key: statico-tools-${{ env.STATICO_VERSION }}

      - name: Install Statico
        run: |
          curl -fsSL "https://github.com/adrianodias8/statico/releases/download/${STATICO_VERSION}/statico-x86_64-unknown-linux-musl.tar.gz" \
            | sudo tar xz -C /usr/local/bin statico
          statico --version

      # Fails the build on broken links, missing anchors and unresolved
      # placeholders. Drop this step if you'd rather deploy regardless.
      - name: Check
        run: statico check

      - name: Build
        run: statico build "$SITE_URL"

      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: "./_site/"

      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4

```

The whole thing takes about 20 seconds, most of which is GitHub spinning up the runner.

3. Enable GitHub Pages in Your Repository:

Go to your repository's Settings tab.

Select Pages from the sidebar.

Under Build and deployment, set the source to GitHub Actions. GitHub will use your configured workflow.

4. (Optional) Add a Custom Domain:

To make your website more professional, you can use a custom domain:

Purchase your domain or use one you already own.

Follow the official GitHub documentation to configure your custom domain.




## 🎉 In Conclusion (or Just Getting Started?)

Remember, simplicity doesn't mean sacrificing style or functionality. Sometimes, stripping things down to basics is exactly what's needed to get moving and for me was how I ended up with my blog running.

Sure, this blog post might just be a test run, but it's my test run. And I'm really happy with it. Though I'll admit "I just want a simple blog" turning into "I wrote my own static site generator" is a very on-brand way for this to have gone.

Stay tuned for more rambling!

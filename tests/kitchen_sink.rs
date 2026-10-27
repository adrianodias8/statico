//! End-to-end: build the kitchen-sink fixture and check every feature.

use statico::build::{BuildOptions, build};
use statico::site::{Mode, Site};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kitchen-sink")
}

fn built(minify: bool) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("site");
    build(
        &fixture(),
        BuildOptions {
            url: Some("https://example.com/".into()),
            out: Some(out.clone()),
            skip_tools: true,
            minify: Some(minify),
            hash_assets: None,
        },
    )
    .unwrap();
    (tmp, out)
}

fn read(out: &Path, p: &str) -> String {
    std::fs::read_to_string(out.join(p)).unwrap_or_else(|_| panic!("missing {p}"))
}

#[test]
fn builds_every_feature() {
    let (_tmp, out) = built(false);

    // Routing
    for p in [
        "index.html",
        "docs/index.html",
        "documentation/index.html",
        "blog/index.html",
        "blog/page/2/index.html",
        "blog/first/index.html",
        "blog/second/index.html",
        "tags/rust/index.html",
        "tags/web/index.html",
        "404.html",
        "robots.txt",
        "assets/images/x.png",
        "sitemap.xml",
        "blog/feed.xml",
        "llms.txt",
        "llms-full.txt",
        "blog/first.md",
    ] {
        assert!(out.join(p).is_file(), "{p} not built");
    }
    assert!(
        !out.join("blog/draft/index.html").exists(),
        "drafts must not be built"
    );
    assert!(!out.join("blog/[content]/index.html").exists());
    assert!(!out.join("tags/[tag]/index.html").exists());

    let index = read(&out, "index.html");
    assert!(
        index.contains("<title>Home · Kitchen Sink</title>"),
        "layout attrs + site data"
    );
    assert!(index.contains("<h1>hello from .env</h1>"), "env");
    assert!(
        !index.contains("sk_test_kitchen_sink"),
        "private .env keys never reach templates"
    );
    assert!(
        index.contains(r#"<a href="https://example.com/" class="active">Home</a>"#),
        "url() + page.url + If inside include loop"
    );
    assert!(
        index.contains("1. Grace (45) (no skills listed)"),
        "YAML collection, orderBy desc, where, If length"
    );
    assert!(
        index.contains("2. Ada (36) #math #engines"),
        "nested loop over item field"
    );
    assert!(!index.contains("Linus"), "where filter");
    assert!(index.contains(r#"data-first="true""#));
    let second = index.find("Second post").unwrap();
    let first = index.find("First post").unwrap();
    assert!(second < first, "content orderBy date desc");
    assert!(!index.contains("Secret draft"));
    assert!(index.contains("First post</a> ★"), "If/Else");
    assert!(
        index.contains("<time>Jan 2024</time> FIRST POST none"),
        "filters"
    );
    assert!(
        index.contains("<h3>Card</h3><span class=\"badge\">New</span>"),
        "nested include with attrs"
    );
    assert!(
        index.contains("<li>Free: 0</li><li>Pro: 12.5</li>"),
        "CSV collection"
    );
    assert!(
        index.contains(r#"<a href="/tags/web/">web (2)</a>"#),
        "taxonomy listing"
    );
    assert!(
        index.contains(r#"const menu = [{"title":"Home""#),
        "collections JSON"
    );
    assert!(
        index.contains("const obj = {open: true}; if (x) {y}"),
        "unknown braces untouched"
    );
    assert!(index.contains(r#"<script type="application/ld+json">{"@context":"https://schema.org","@type":"WebSite""#), "seo on home");

    let docs = read(&out, "docs/index.html");
    assert!(
        docs.contains("<aside>docs sidebar</aside>")
            && docs.contains("<title>Docs · Kitchen Sink</title>"),
        "nested layouts"
    );
    assert!(docs.contains(r#"/docs/" class="active""#));
    // `<page src>` renders the same page; only URL-dependent bits differ.
    let doc2 = read(&out, "documentation/index.html");
    assert!(
        doc2.contains("<aside>docs sidebar</aside>")
            && doc2.contains("https://example.com/documentation/")
    );

    let post = read(&out, "blog/first/index.html");
    assert!(post.contains("<title>First post · Kitchen Sink</title>"));
    assert!(
        post.contains(r#"<p class="featured">Featured</p>"#),
        "front matter condition"
    );
    assert!(
        post.contains(r#"<h2 id="getting-started-1">"#),
        "unique heading ids"
    );
    assert!(post.contains(r##"<a href="#getting-started">"##), "[toc]");
    assert!(post.contains("<table>"));
    assert!(
        post.contains("{post.title} and {frontmatter.title} stay as written"),
        "markdown is not re-templated"
    );
    assert!(
        post.contains(r#"<pre data-lang="html" style="#),
        "syntax highlighting"
    );
    assert!(
        !post.contains("<div class=\"card\">"),
        "code blocks are not processed"
    );
    assert!(post.contains("window.frontmatter = {"), "window globals");
    assert!(post.contains(r#"id="static-content""#));
    assert!(
        post.contains(r#"<meta name="description" content="The very first post">"#),
        "seo description"
    );
    assert!(post.contains(r#"<link rel="canonical" href="https://example.com/blog/first/">"#));
    assert!(post.contains(r#"<meta property="og:type" content="article">"#));
    assert!(
        post.contains(r#"<link rel="alternate" type="application/atom+xml""#),
        "feed link"
    );
    assert!(post.contains(r#""@type":"BlogPosting""#));

    let second = read(&out, "blog/second/index.html");
    assert!(
        second.contains(r#"<span class="badge">inline</span>"#),
        "includes inside markdown"
    );

    // Pagination
    let p1 = read(&out, "blog/index.html");
    assert!(p1.contains("<h2>Second post</h2>") && !p1.contains("<h2>First post</h2>"));
    assert!(
        p1.contains(r#"Page 1 of 2 <a href="/blog/page/2/">Older</a>"#),
        "{p1}"
    );
    let p2 = read(&out, "blog/page/2/index.html");
    assert!(
        p2.contains("<h2>First post</h2>")
            && p2.contains(r#"Page 2 of 2 <a href="/blog/">Newer</a>"#)
    );

    // Taxonomies
    let rust = read(&out, "tags/rust/index.html");
    assert!(
        rust.contains("<h1>Rust (1)</h1>")
            && rust.contains("First post")
            && !rust.contains("Second post")
    );
    assert!(rust.contains("<title>Tagged Rust · Kitchen Sink</title>"));

    // Feeds, sitemap, llms
    let feed = read(&out, "blog/feed.xml");
    assert!(feed.contains("<link href=\"https://example.com/blog/first/\"/>"));
    assert!(
        feed.find("Second post").unwrap() < feed.find("First post").unwrap(),
        "newest first"
    );
    assert!(!feed.contains("Secret draft"));
    let sitemap = read(&out, "sitemap.xml");
    assert!(sitemap.contains("<loc>https://example.com/blog/first/</loc>"));
    assert!(sitemap.contains("<loc>https://example.com/blog/page/2/</loc>"));
    assert!(!sitemap.contains("404"));
    let llms = read(&out, "llms.txt");
    assert!(
        llms.starts_with("# Kitchen Sink\n\n> Every Statico feature on one site\n"),
        "{llms}"
    );
    assert!(
        llms.contains("- [First post](https://example.com/blog/first.md): The very first post"),
        "{llms}"
    );
    assert!(read(&out, "blog/first.md").starts_with("# First post\n\n[toc]"));
    assert!(read(&out, "llms-full.txt").contains("URL: https://example.com/blog/second/"));

    assert!(read(&out, "404.html").contains("Nothing at /404.html"));
}

#[test]
fn production_defaults_minify_and_optimize() {
    let (_tmp, out) = built(true);
    let index = read(&out, "index.html");
    assert!(!index.contains("\n    <title>"), "minified");
    assert!(index.contains("<h1>hello from .env</h1>"));
}

#[test]
fn dev_mode_shows_drafts_and_resolves_routes() {
    let site = Site::load(&fixture(), Mode::Dev, None).unwrap();
    let (src, page) = site.resolve("/blog/draft").expect("draft route in dev");
    assert_eq!(page, 1);
    let r = site.render(&src, "/blog/draft/", 1).unwrap();
    assert!(r.html.contains("Not ready."));
    assert!(site.resolve("/blog/[content]").is_none());
    assert!(site.resolve("/../etc/passwd").is_none());
    assert!(site.resolve("/docs.html").is_some());
    assert_eq!(site.resolve("/blog/page/2/").map(|(_, p)| p), Some(2));
    assert!(site.resolve("/tags/rust/").is_some());
    assert!(site.resolve("/tags/nope/").is_none());
    let home = site.render(&site.resolve("/").unwrap().0, "/", 1).unwrap();
    assert!(home.html.contains("hello from .env"));
    // Page 3 of a 3-post blog (draft included in dev) exists, page 9 doesn't.
    let (blog, _) = site.resolve("/blog/").unwrap();
    assert!(site.render(&blog, "/blog/", 3).is_ok());
    assert!(site.render(&blog, "/blog/", 9).is_err());
}

#[test]
fn schema_violations_fail_the_build() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("p");
    copy(&fixture(), &root);
    std::fs::write(root.join("content/blog/bad.md"), "---\ntitle: 5\n---\nx").unwrap();
    let err = build(
        &root,
        BuildOptions {
            skip_tools: true,
            out: Some(tmp.path().join("o")),
            ..Default::default()
        },
    )
    .err()
    .unwrap();
    let be = err
        .downcast_ref::<statico::build::BuildError>()
        .expect("structured errors");
    let e = &be.errors[0];
    assert_eq!(e.code, Some("S201"));
    assert_eq!(e.file.as_deref(), Some("content/blog/bad.md"));
    assert!(
        e.message.contains("`title` must be a string")
            && e.message.contains("missing required field `date`"),
        "{}",
        e.message
    );
}

#[test]
fn check_finds_broken_links_and_missing_alt() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("p");
    copy(&fixture(), &root);
    let ok = statico::check::check(&root, None, false).unwrap();
    assert!(ok.ok, "fixture should be clean: {:?}", ok.errors);
    std::fs::write(
        root.join("pages/broken.html"),
        "<layout src=\"base.html\" title=\"B\"><a href=\"/nope/\">x</a><a href=\"/blog/first/#missing\">y</a><a href=\"/blog/first/#getting-started\">z</a><img src=\"/assets/images/x.png\"></layout>",
    )
    .unwrap();
    let r = statico::check::check(&root, None, false).unwrap();
    assert!(!r.ok);
    let codes: Vec<_> = r.errors.iter().filter_map(|e| e.code).collect();
    assert_eq!(codes, vec!["S101", "S105"], "{:?}", r.errors);
    assert!(r.warnings.iter().any(|w| w.code == Some("S102")));
}

#[test]
fn private_env_stays_out_of_the_output() {
    const SECRET: &str = "sk_test_kitchen_sink_do_not_publish";
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("p");
    copy(&fixture(), &root);
    let page = root.join("pages/env.html");

    // Asking for a private key renders nothing secret and says why, once.
    std::fs::write(
        &page,
        "<layout src=\"base.html\" title=\"E\"><p>{env.API_SECRET}</p></layout>",
    )
    .unwrap();
    let r = statico::check::check(&root, None, false).unwrap();
    assert!(r.ok, "{:?}", r.errors);
    let codes: Vec<_> = r.warnings.iter().filter_map(|w| w.code).collect();
    assert!(codes.contains(&"S109"), "{:?}", r.warnings);
    assert!(
        !r.warnings
            .iter()
            .any(|w| w.code == Some("S106") && w.message.contains("env.")),
        "reported as S109 only: {:?}",
        r.warnings
    );

    // A secret copied into a page by hand fails the check wherever it lands,
    // and the report never repeats the value.
    std::fs::write(
        &page,
        format!("<layout src=\"base.html\" title=\"E\"><p>{SECRET}</p></layout>"),
    )
    .unwrap();
    let r = statico::check::check(&root, None, false).unwrap();
    assert!(!r.ok);
    assert!(
        r.errors.iter().all(|e| e.code == Some("S108")),
        "{:?}",
        r.errors
    );
    assert!(
        r.errors
            .iter()
            .any(|e| e.url.as_deref() == Some("/env/index.html")),
        "{:?}",
        r.errors
    );
    assert!(r.errors.iter().all(|e| !e.message.contains(SECRET)));
}

#[test]
fn refuses_to_clean_the_project() {
    let err = build(
        &fixture(),
        BuildOptions {
            out: Some(fixture()),
            skip_tools: true,
            ..Default::default()
        },
    )
    .expect_err("must refuse");
    assert!(err.to_string().contains("refusing"), "{err}");
    assert!(fixture().join("pages/index.html").exists());
}

fn copy(from: &Path, to: &Path) {
    for e in walkdir::WalkDir::new(from) {
        let e = e.unwrap();
        let rel = e.path().strip_prefix(from).unwrap();
        let t = to.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&t).unwrap();
        } else {
            std::fs::copy(e.path(), &t).unwrap();
        }
    }
}

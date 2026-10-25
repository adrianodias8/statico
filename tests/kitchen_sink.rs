//! End-to-end: build the kitchen-sink fixture and check every feature.

use static_rs::build::{BuildOptions, build};
use static_rs::site::{Mode, Site};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kitchen-sink")
}

fn built() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("site");
    build(
        &fixture(),
        BuildOptions {
            url: Some("https://example.com/".into()),
            out: Some(out.clone()),
            skip_tools: true,
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
    let (_tmp, out) = built();

    // Routing
    for p in [
        "index.html",
        "docs/index.html",
        "documentation/index.html",
        "blog/index.html",
        "blog/first/index.html",
        "blog/second/index.html",
        "404.html",
        "robots.txt",
        "assets/images/x.png",
        "sitemap.xml",
    ] {
        assert!(out.join(p).is_file(), "{p} not built");
    }
    assert!(
        !out.join("blog/draft/index.html").exists(),
        "drafts must not be built"
    );
    assert!(!out.join("blog/[content]/index.html").exists());

    let index = read(&out, "index.html");
    assert!(
        index.contains("<title>Home · Kitchen Sink</title>"),
        "layout attrs + site data"
    );
    assert!(index.contains("<h1>hello from .env</h1>"), "env");
    assert!(
        index.contains(r#"<a href="https://example.com/" class="active">Home</a>"#),
        "url() + page.url + If inside include loop"
    );
    assert!(
        index.contains("1. Grace (45) (no skills listed)"),
        "orderBy desc, where, If length"
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
        index.contains("<h3>Card</h3><span class=\"badge\">New</span>"),
        "nested include with attrs"
    );
    assert!(
        index.contains(r#"const menu = [{"title":"Home""#),
        "collections JSON"
    );
    assert!(
        index.contains("const obj = {open: true}; if (x) {y}"),
        "unknown braces untouched"
    );

    let docs = read(&out, "docs/index.html");
    assert!(
        docs.contains("<aside>docs sidebar</aside>")
            && docs.contains("<title>Docs · Kitchen Sink</title>"),
        "nested layouts"
    );
    assert!(docs.contains(r#"/docs/" class="active""#));
    // `<page src>` renders the same page (only the active nav link differs by URL).
    assert_eq!(
        docs.replace(r#" class="active""#, ""),
        read(&out, "documentation/index.html"),
        "<page src>"
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
        post.contains("&lt;include src=\"card.html\"&gt;"),
        "code blocks untouched"
    );
    assert!(post.contains("window.frontmatter = {"), "window globals");
    assert!(post.contains(r#"id="static-content""#));

    let second = read(&out, "blog/second/index.html");
    assert!(
        second.contains(r#"<span class="badge">inline</span>"#),
        "includes inside markdown"
    );

    let list = read(&out, "blog/index.html");
    assert!(
        list.contains("<h2>First post</h2>") && list.contains("<table>"),
        "post.html field"
    );

    assert!(read(&out, "404.html").contains("Nothing at /404.html"));
    let sitemap = read(&out, "sitemap.xml");
    assert!(sitemap.contains("<loc>https://example.com/blog/first/</loc>"));
    assert!(!sitemap.contains("404"));
}

#[test]
fn dev_mode_shows_drafts_and_resolves_routes() {
    let site = Site::load(&fixture(), Mode::Dev, None).unwrap();
    let src = site.resolve("/blog/draft").expect("draft route in dev");
    let html = site.render(&src, "/blog/draft/").unwrap();
    assert!(html.contains("Not ready."));
    assert!(site.resolve("/blog/[content]").is_none());
    assert!(site.resolve("/../etc/passwd").is_none());
    assert!(site.resolve("/docs.html").is_some());
    // Dev uses the in-browser Tailwind build for {tailwindcss}; this fixture has none.
    let home = site.render(&site.resolve("/").unwrap(), "/").unwrap();
    assert!(home.contains("hello from .env"));
}

#[test]
fn refuses_to_clean_the_project() {
    let err = build(
        &fixture(),
        BuildOptions {
            url: None,
            out: Some(fixture()),
            skip_tools: true,
        },
    )
    .err()
    .expect("must refuse");
    assert!(err.to_string().contains("refusing"), "{err}");
    assert!(fixture().join("pages/index.html").exists());
}

//! `statico dev` and `statico serve`.
//!
//! The dev server renders pages on request, watches the project and reloads
//! the browser over Server-Sent Events (no second live-reload port, no
//! browser extension). Render errors are shown in the browser as an overlay
//! and clear themselves when the file is fixed.

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode, Uri, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use notify::{RecursiveMode, Watcher};
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

use crate::assets;
use crate::site::{Mode, Site, insert_before, sanitize_url_path};

const LIVERELOAD_PATH: &str = "/__statico/livereload";
const LIVERELOAD_SCRIPT: &str = r#"<script>(()=>{let down=false;const es=new EventSource('/__statico/livereload');es.onmessage=e=>{if(e.data==='reload')location.reload()};es.onerror=()=>{down=true};es.onopen=()=>{if(down)location.reload()}})()</script>"#;
pub const DEFAULT_404: &str = include_str!("404.html");

pub struct DevOptions {
    pub url: Option<String>,
    pub port: Option<u16>,
    pub host: String,
    pub open: bool,
}

struct DevState {
    root: PathBuf,
    url: Option<String>,
    site: RwLock<Arc<Site>>,
    tx: broadcast::Sender<()>,
    /// Where `esbuild --watch` writes bundled JS during development.
    cache_dir: PathBuf,
}

impl DevState {
    fn site(&self) -> Arc<Site> {
        self.site.read().unwrap().clone()
    }

    fn reload(&self) {
        match Site::load(&self.root, Mode::Dev, self.url.clone()) {
            Ok(s) => *self.site.write().unwrap() = Arc::new(s),
            Err(e) => eprintln!("error: {e:#}"),
        }
        let _ = self.tx.send(());
    }
}

async fn bind(host: &str, start: u16) -> Result<(tokio::net::TcpListener, u16)> {
    for port in start..start.saturating_add(50) {
        if let Ok(l) = tokio::net::TcpListener::bind((host, port)).await {
            return Ok((l, port));
        }
    }
    anyhow::bail!("no free port between {start} and {}", start + 49)
}

pub fn dev(root: &Path, opts: DevOptions) -> Result<()> {
    let site = Site::load(root, Mode::Dev, opts.url.clone())?;
    let root = site.root.clone();
    let port = opts.port.or(site.config.dev.port).unwrap_or(3000);

    let cache_dir =
        std::env::temp_dir().join(format!("statico-dev-{:x}", hash(&root.display().to_string())));
    let _ = std::fs::remove_dir_all(&cache_dir);
    std::fs::create_dir_all(cache_dir.join("assets/js"))?;

    let (tx, _) = broadcast::channel(16);
    let state = Arc::new(DevState {
        root: root.clone(),
        url: opts.url.clone(),
        site: RwLock::new(Arc::new(site)),
        tx,
        cache_dir: cache_dir.clone(),
    });

    // Bundle JS continuously, like the original's `esbuild --watch`.
    let esbuild: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    if root.join("assets/js/main.js").is_file()
        && let Some(tool) = assets::find_esbuild(&root)
    {
        let mut child = tool.command(&root);
        child
            .arg("assets/js/main.js")
            .arg("--bundle")
            .arg(format!(
                "--outfile={}",
                cache_dir.join("assets/js/main.js").display()
            ))
            .arg("--watch=forever")
            .arg("--log-level=warning")
            .stdin(std::process::Stdio::null());
        #[cfg(unix)]
        {
            // Own process group, so `npx` *and* the esbuild it spawns are stopped together.
            use std::os::unix::process::CommandExt;
            child.process_group(0);
        }
        let child = child.spawn();
        match child {
            Ok(c) => *esbuild.lock().unwrap() = Some(c),
            Err(e) => eprintln!("warning: could not start {}: {e}", tool.label),
        }
    }

    // Watch the project. Only real changes count (not reads/metadata), and
    // bursts (editor saves, git checkouts) are coalesced into one reload.
    let watch_state = state.clone();
    let out_dir = root.join("_site");
    let (ev_tx, ev_rx) = std::sync::mpsc::channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        use notify::EventKind::*;
        let Ok(ev) = res else { return };
        let real = match ev.kind {
            Create(_) | Remove(_) => true,
            Modify(notify::event::ModifyKind::Metadata(_)) => false,
            Modify(_) => true,
            _ => false,
        };
        if real {
            for p in ev.paths {
                let _ = ev_tx.send(p);
            }
        }
    })?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .context("cannot watch project files")?;
    let _ = watcher.watch(&cache_dir, RecursiveMode::Recursive);
    std::thread::spawn(move || {
        while let Ok(first) = ev_rx.recv() {
            let mut batch = vec![first];
            while let Ok(p) = ev_rx.recv_timeout(Duration::from_millis(60)) {
                batch.push(p);
            }
            batch.retain(|p| is_relevant(p, &watch_state.root, &out_dir, &watch_state.cache_dir));
            batch.dedup();
            if let Some(first) = batch.first() {
                let shown = first.strip_prefix(&watch_state.root).unwrap_or(first);
                let more = if batch.len() > 1 {
                    format!(" (+{} more)", batch.len() - 1)
                } else {
                    String::new()
                };
                println!("  ↻ {}{more}", shown.display());
                watch_state.reload();
            }
        }
    });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let (listener, port) = bind(&opts.host, port).await?;
        let url = format!("http://{}:{port}", if opts.host == "0.0.0.0" { "localhost" } else { &opts.host });
        println!("\n  ⚡ Statico dev server running at {url}\n     Watching for changes. Press Ctrl+C to stop.\n");
        if opts.open {
            let _ = open::that(&url);
        }
        let app = Router::new()
            .route(LIVERELOAD_PATH, get(livereload))
            .fallback(get(handle))
            .with_state(state);
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        anyhow::Ok(())
    })?;

    if let Some(c) = esbuild.lock().unwrap().take() {
        kill_tree(c);
    }
    let _ = std::fs::remove_dir_all(&cache_dir);
    drop(watcher);
    println!("\n  Bye 👋");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("signal handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn kill_tree(mut child: Child) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill")
            .args(["-s", "TERM", "--"])
            .arg(format!("-{}", child.id()))
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn is_relevant(p: &Path, root: &Path, out_dir: &Path, cache_dir: &Path) -> bool {
    if p.starts_with(cache_dir) {
        return true;
    }
    if p.starts_with(out_dir) {
        return false;
    }
    let Ok(rel) = p.strip_prefix(root) else {
        return false;
    };
    let name = rel.to_string_lossy();
    if name == ".env" {
        return true;
    }
    !rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "node_modules" || s == "_site" || s == "target" || (s.starts_with('.'))
    }) && !name.ends_with('~')
        && !name.ends_with(".swp")
}

async fn livereload(
    State(st): State<Arc<DevState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(st.tx.subscribe())
        .filter_map(|m| m.ok().map(|_| Ok(Event::default().data("reload"))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn handle(
    State(st): State<Arc<DevState>>,
    uri: Uri,
    headers: axum::http::HeaderMap,
) -> Response {
    let path = uri.path().to_string();
    // Agents can ask for markdown: `Accept: text/markdown`.
    let wants_md = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/markdown"));
    let res = tokio::task::spawn_blocking(move || serve_path(&st, &path, wants_md)).await;
    match res {
        Ok(r) => r,
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Join a URL path onto a directory without allowing traversal.
fn safe_join(base: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = crate::site::percent_decode(url_path.split(['?', '#']).next()?);
    let mut p = base.to_path_buf();
    for seg in decoded.split('/') {
        match seg {
            "" | "." => {}
            ".." => return None,
            s if s.contains('\\') || s.contains('\0') => return None,
            s => p.push(s),
        }
    }
    Some(p)
}

fn file_response(path: &Path) -> Option<Response> {
    let bytes = std::fs::read(path).ok()?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(
        Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(bytes))
            .unwrap(),
    )
}

fn html_response(status: StatusCode, html: String, site: &Site) -> Response {
    let html = insert_before(&html, "</body>", LIVERELOAD_SCRIPT);
    let mut r = (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        html,
    )
        .into_response();
    add_headers(&mut r, site);
    r
}

fn add_headers(r: &mut Response, site: &Site) {
    for (k, v) in &site.config.headers {
        if let (Ok(k), Ok(v)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string())
                    .as_str(),
            ),
        ) {
            r.headers_mut().insert(k, v);
        }
    }
}

fn serve_path(st: &DevState, path: &str, wants_md: bool) -> Response {
    let site = st.site();

    // Generated files: feeds.
    for feed in crate::feeds::configs(&site) {
        if feed.url == path {
            return match crate::feeds::atom(&site, &feed) {
                Ok(xml) => (
                    [(header::CONTENT_TYPE, "application/atom+xml; charset=utf-8")],
                    xml,
                )
                    .into_response(),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")).into_response(),
            };
        }
    }

    // Plain files: bundled JS, then assets/, then public/.
    if let Some(sub) = path.strip_prefix("/assets/") {
        for base in [st.cache_dir.join("assets"), site.root.join("assets")] {
            if let Some(f) = safe_join(&base, sub).filter(|f| f.is_file())
                && let Some(mut r) = file_response(&f)
            {
                add_headers(&mut r, &site);
                return r;
            }
        }
    }
    if let Some(f) = safe_join(&site.root.join("public"), path).filter(|f| f.is_file())
        && let Some(mut r) = file_response(&f)
    {
        add_headers(&mut r, &site);
        return r;
    }

    if sanitize_url_path(path).is_none() {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }

    // `/blog/post.md` → markdown source of /blog/post/ (like the build's mirrors).
    let (lookup, md_request) = match path.strip_suffix(".md") {
        Some(p) => (p.to_string(), true),
        None => (path.to_string(), wants_md),
    };

    if let Some((source, page)) = site.resolve(&lookup) {
        let started = std::time::Instant::now();
        let clean_lookup = sanitize_url_path(&lookup).unwrap_or_default();
        let canonical = if clean_lookup.is_empty() {
            "/".to_string()
        } else {
            format!("/{clean_lookup}/")
        };
        let base = if page > 1 {
            canonical
                .trim_end_matches(&format!("page/{page}/"))
                .to_string()
        } else {
            canonical
        };
        let result = site.render(&source, &base, page);
        // `/blog/page/9/` past the last page is a 404, not an error.
        let out_of_range = page > 1
            && result
                .as_ref()
                .err()
                .is_some_and(|e| e.to_string().contains("does not exist (only"));
        if !out_of_range {
            return match result {
                Ok(r) if md_request => match r.markdown {
                    Some(md) => (
                        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
                        format!("# {}\n\n{md}\n", r.title),
                    )
                        .into_response(),
                    None => (
                        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
                        format!(
                            "# {}\n\n{}\n",
                            r.title,
                            crate::postprocess::main_text(&r.html)
                        ),
                    )
                        .into_response(),
                },
                Ok(r) => {
                    println!("  200 {path} ({:.1?})", started.elapsed());
                    html_response(StatusCode::OK, r.html, &site)
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    eprintln!("  500 {path}\n  error: {msg}");
                    html_response(StatusCode::INTERNAL_SERVER_ERROR, error_page(&msg), &site)
                }
            };
        }
    }

    // 404: the project's own pages/404.html, else the built-in page.
    let custom = site.root.join("pages/404.html");
    let html = if custom.is_file() {
        site.render(&crate::site::Source::Page(custom), "/404.html", 1)
            .map(|r| r.html)
            .unwrap_or_else(|e| error_page(&format!("{e:#}")))
    } else {
        DEFAULT_404.to_string()
    };
    if !path.ends_with(".map") && !path.ends_with("favicon.ico") {
        println!("  404 {path}");
    }
    html_response(StatusCode::NOT_FOUND, html, &site)
}

fn error_page(msg: &str) -> String {
    let esc = msg
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Statico · Build error</title>
<style>body{{margin:0;font:15px/1.5 ui-sans-serif,system-ui,sans-serif;background:#0b0b0c;color:#e5e5e5}}
main{{max-width:860px;margin:10vh auto;padding:0 24px}}h1{{font-size:18px;color:#f87171;margin:0 0 16px}}
pre{{white-space:pre-wrap;background:#18181b;border:1px solid #3f3f46;border-radius:10px;padding:18px;font:13px/1.6 ui-monospace,monospace;color:#fafafa}}
p{{color:#a1a1aa;font-size:13px}}</style></head><body><main><h1>⚠ Couldn't render this page</h1><pre>{esc}</pre>
<p>Fix the file and save. This page reloads automatically.</p></main></body></html>"#
    )
}

/// `statico build --watch`: rebuild into the output folder on every change.
pub fn watch_build(root: &Path, opts: impl Fn() -> crate::build::BuildOptions) -> Result<()> {
    let run = || match crate::build::build(root, opts()) {
        Ok(r) => Some(r.out_dir),
        Err(e) => {
            eprintln!("error: {e:#}");
            None
        }
    };
    let mut out_dir = run().unwrap_or_else(|| root.join("_site"));
    let root = root.canonicalize()?;
    let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        use notify::EventKind::*;
        if let Ok(ev) = res
            && matches!(ev.kind, Create(_) | Remove(_) | Modify(_))
            && !matches!(ev.kind, Modify(notify::event::ModifyKind::Metadata(_)))
        {
            for p in ev.paths {
                let _ = tx.send(p);
            }
        }
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    println!("  Watching for changes (Ctrl+C to stop)…");
    let none = PathBuf::from("/nonexistent-statico-cache");
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first];
        while let Ok(p) = rx.recv_timeout(Duration::from_millis(100)) {
            batch.push(p);
        }
        if batch.iter().any(|p| is_relevant(p, &root, &out_dir, &none)) {
            if let Some(d) = run() {
                out_dir = d;
            }
            // Drain events caused by our own writes.
            while rx.recv_timeout(Duration::from_millis(50)).is_ok() {}
        }
    }
    Ok(())
}

/// `statico serve`: preview a finished build exactly as a static host would.
pub fn serve(dir: &Path, host: &str, port: u16, open_browser: bool) -> Result<()> {
    use tower_http::services::{ServeDir, ServeFile};
    if !dir.is_dir() {
        anyhow::bail!("{} does not exist. Run `statico build` first.", dir.display());
    }
    let svc = ServeDir::new(dir).not_found_service(ServeFile::new(dir.join("404.html")));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let (listener, port) = bind(host, port).await?;
        let url = format!(
            "http://{}:{port}",
            if host == "0.0.0.0" { "localhost" } else { host }
        );
        println!("\n  Serving {} at {url}  (Ctrl+C to stop)\n", dir.display());
        if open_browser {
            let _ = open::that(&url);
        }
        axum::serve(listener, Router::new().fallback_service(svc))
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        anyhow::Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_blocks_traversal() {
        let b = Path::new("/srv/public");
        assert_eq!(
            safe_join(b, "/a/b.png"),
            Some(PathBuf::from("/srv/public/a/b.png"))
        );
        assert_eq!(safe_join(b, "/../etc/passwd"), None);
        assert_eq!(safe_join(b, "/%2e%2e/etc/passwd"), None);
    }
}

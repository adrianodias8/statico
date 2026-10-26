use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};

use forno::{build, check, diag, docs, new, say, scaffold};

#[derive(Parser)]
#[command(
    name = "forno",
    version,
    about = "🔥 Forno — bake plain HTML into fast static sites.",
    after_help = "Docs: `forno docs` (offline reference) · `forno explain <code>` · add --json for machine-readable output"
)]
struct Cli {
    /// Project directory (defaults to the current directory)
    #[arg(short = 'C', long = "dir", global = true, default_value = ".")]
    dir: PathBuf,
    /// Print one machine-readable JSON object to stdout (for scripts and AI agents)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new site from a template
    New {
        /// Folder to create
        folder: String,
        /// Template: starter, aria, boxed, blog, stone, owner/repo, a GitHub URL, a local folder or .zip
        #[arg(short, long, default_value = "starter")]
        template: String,
        /// Don't run `npm install` afterwards
        #[arg(long)]
        no_install: bool,
        /// Start the dev server (and open the browser) when done
        #[arg(long)]
        dev: bool,
    },
    /// Turn the current (or -C) folder into a minimal Forno project
    Init {
        /// Site name (default: the folder name)
        #[arg(long)]
        name: Option<String>,
    },
    /// Start the development server with live reload
    Dev {
        /// Base URL for `{ url('/path') }` (default: relative)
        url: Option<String>,
        #[arg(short, long)]
        port: Option<u16>,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Open the browser
        #[arg(short, long)]
        open: bool,
    },
    /// Build the production site (into `_site` by default)
    Build {
        /// Base URL for absolute links, feeds and sitemap.xml (default: relative)
        url: Option<String>,
        /// Output directory
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Don't run Tailwind or esbuild
        #[arg(long)]
        skip_tools: bool,
        /// Don't minify HTML
        #[arg(long)]
        no_minify: bool,
        /// Rebuild whenever a file changes
        #[arg(short, long)]
        watch: bool,
    },
    /// Serve the built site locally, exactly as a static host would
    Serve {
        #[arg(short, long, default_value_t = 4000)]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Directory to serve (default: the build directory)
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        open: bool,
    },
    /// Build into a temp folder and check links, anchors, alt text, titles
    Check {
        /// Base URL (only affects absolute links)
        url: Option<String>,
        /// Also fail on warnings, and check meta descriptions and duplicate titles
        #[arg(long)]
        strict: bool,
    },
    /// Create a page, post, include, layout or collection
    Add {
        #[command(subcommand)]
        what: AddCmd,
    },
    /// List every URL and the file that renders it
    Routes,
    /// Show the reference: `forno docs`, `forno docs loops`, `forno docs --all`
    Docs {
        topic: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Explain an error code, e.g. `forno explain F101`
    Explain { code: String },
    /// Compare two build output folders (e.g. before/after an upgrade)
    Diff { old: PathBuf, new: PathBuf },
    /// Print the JSON Schema for forno.json
    Schema,
    /// Write AGENTS.md (and CLAUDE.md) describing this project for AI coding agents
    Agents,
    /// List the official templates
    Templates,
}

#[derive(Subcommand)]
enum AddCmd {
    /// pages/<path>.html using your main layout
    Page {
        path: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        layout: Option<String>,
    },
    /// content/<folder>/<slug>.md (plus a template and listing page if missing)
    Post {
        folder: String,
        title: String,
        #[arg(long)]
        draft: bool,
    },
    /// includes/<name>.html
    Include { name: String },
    /// layouts/<name>.html
    Layout { name: String },
    /// collections/<name>.json
    Collection { name: String },
}

/// Accept the original Static CLI's `new my-site --blog` form.
fn legacy_args() -> Vec<String> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "new") {
        for a in args.iter_mut().skip(2) {
            if let Some(slug) = a.strip_prefix("--")
                && new::TEMPLATES.iter().any(|t| t.slug == slug)
            {
                *a = format!("--template={slug}");
            }
        }
    }
    args
}

fn print_json(v: &impl serde::Serialize) {
    println!(
        "{}",
        serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into())
    );
}

fn created(json_mode: bool, root: &Path, files: Vec<String>) {
    if json_mode {
        print_json(&json!({"ok": true, "created": files, "root": root}));
    } else {
        for f in files {
            println!("  + {f}");
        }
    }
}

fn print_diags(list: &[diag::Diagnostic]) {
    for d in list {
        let loc = match (&d.file, d.line, &d.url) {
            (Some(f), Some(l), _) => format!("{f}:{l}: "),
            (Some(f), None, Some(u)) => format!("{f} ({u}): "),
            (Some(f), None, None) => format!("{f}: "),
            _ => String::new(),
        };
        let code = d.code.map(|c| format!(" [{c}]")).unwrap_or_default();
        let icon = if d.level == "error" { "✗" } else { "!" };
        println!("  {icon} {loc}{}{code}", d.message);
    }
}

fn run(cli: Cli) -> Result<i32> {
    let j = cli.json;
    match cli.cmd {
        Cmd::New {
            folder,
            template,
            no_install,
            dev,
        } => {
            let path = new::new_project(&folder, &template, !no_install)?;
            if !path.join("AGENTS.md").exists() {
                let _ = scaffold::write_agents(&path);
            }
            if j {
                print_json(&json!({"ok": true, "path": path}));
            }
            if dev {
                forno::dev::dev(
                    &path,
                    forno::dev::DevOptions {
                        url: None,
                        port: None,
                        host: "127.0.0.1".into(),
                        open: true,
                    },
                )?;
            }
        }
        Cmd::Init { name } => {
            let files = scaffold::init(&cli.dir, name.as_deref())?;
            created(j, &cli.dir, files);
            say!("\nNext: forno dev");
        }
        Cmd::Dev {
            url,
            port,
            host,
            open,
        } => forno::dev::dev(
            &cli.dir,
            forno::dev::DevOptions {
                url,
                port,
                host,
                open,
            },
        )?,
        Cmd::Build {
            url,
            out,
            skip_tools,
            no_minify,
            watch,
        } => {
            let opts = || build::BuildOptions {
                url: url.clone(),
                out: out.clone(),
                skip_tools,
                minify: no_minify.then_some(false),
                hash_assets: None,
            };
            if watch {
                return forno::dev::watch_build(&cli.dir, opts).map(|_| 0);
            }
            let report = build::build(&cli.dir, opts())?;
            if j {
                print_json(&report);
            }
        }
        Cmd::Serve {
            port,
            host,
            out,
            open,
        } => {
            let dir = match out {
                Some(o) => o,
                None => {
                    let site = forno::site::Site::load(&cli.dir, forno::site::Mode::Build, None)?;
                    build::output_dir(&site, None)
                }
            };
            forno::dev::serve(&dir, &host, port, open)?;
        }
        Cmd::Check { url, strict } => {
            let r = check::check(&cli.dir, url, strict)?;
            if j {
                print_json(&r);
            } else {
                print_diags(&r.errors);
                print_diags(&r.warnings);
                println!(
                    "\n{} {} page(s), {} link(s) checked: {} error(s), {} warning(s)",
                    if r.ok { "✓" } else { "✗" },
                    r.pages,
                    r.links_checked,
                    r.errors.len(),
                    r.warnings.len()
                );
                if !r.errors.is_empty() || !r.warnings.is_empty() {
                    println!("  Run `forno explain <code>` for details on any code.");
                }
            }
            return Ok(if r.ok { 0 } else { 1 });
        }
        Cmd::Add { what } => {
            let root = &cli.dir;
            if !matches!(what, AddCmd::Layout { .. }) && !root.join("pages").is_dir() {
                bail!("no `pages` folder here; run `forno init` first (or use -C <dir>)");
            }
            let files = match what {
                AddCmd::Page {
                    path,
                    title,
                    layout,
                } => scaffold::add_page(root, &path, title.as_deref(), layout.as_deref())?,
                AddCmd::Post {
                    folder,
                    title,
                    draft,
                } => {
                    let schema = forno::config::Config::load(root)?
                        .schemas
                        .get(folder.trim_matches('/'))
                        .cloned();
                    scaffold::add_post(root, &folder, &title, draft, schema.as_ref())?
                }
                AddCmd::Include { name } => scaffold::add_include(root, &name)?,
                AddCmd::Layout { name } => scaffold::add_layout(root, &name)?,
                AddCmd::Collection { name } => scaffold::add_collection(root, &name)?,
            };
            created(j, root, files);
        }
        Cmd::Routes => {
            let site = forno::site::Site::load(&cli.dir, forno::site::Mode::Dev, None)?;
            let routes = site.routes()?;
            if j {
                let list: Vec<_> = routes
                    .iter()
                    .map(|r| {
                        let kind = match r.source {
                            forno::site::Source::Page(_) => "page",
                            forno::site::Source::Content(_) => "content",
                            forno::site::Source::Term { .. } => "term",
                        };
                        json!({"url": r.url, "source": site.rel(r.source.path()), "kind": kind, "output": r.out})
                    })
                    .collect();
                print_json(&json!({"ok": true, "routes": list}));
            } else {
                let w = routes.iter().map(|r| r.url.len()).max().unwrap_or(0);
                for r in &routes {
                    println!("  {:<w$}  {}", r.url, site.rel(r.source.path()));
                }
            }
        }
        Cmd::Docs { topic, all } => {
            if all {
                print!("{}", docs::REFERENCE);
            } else if let Some(t) = topic {
                match docs::topic(&t) {
                    Some(text) => print!("{text}"),
                    None => {
                        let names: Vec<_> = docs::topics().into_iter().map(|(n, _)| n).collect();
                        bail!("no topic `{t}`. Topics: {}", names.join(", "));
                    }
                }
            } else {
                println!("Forno reference — `forno docs <topic>` or `forno docs --all`\n");
                for (name, _) in docs::topics() {
                    println!("  {name:<13} {}", docs::summary(name));
                }
            }
        }
        Cmd::Explain { code } => match docs::explain(&code) {
            Some(c) if j => {
                print_json(&json!({"ok": true, "code": c.code, "title": c.title, "fix": c.fix}))
            }
            Some(c) => println!("{}: {}\n\n{}", c.code, c.title, c.fix),
            None => bail!("unknown code `{code}`; see `forno docs errors`"),
        },
        Cmd::Diff { old, new } => {
            let r = forno::diff::diff(&old, &new)?;
            if j {
                print_json(&r);
            } else {
                for f in &r.added {
                    println!("  + {f}");
                }
                for f in &r.removed {
                    println!("  - {f}");
                }
                for f in &r.changed {
                    println!("  ~ {f}");
                }
                println!(
                    "\n{} added, {} removed, {} changed, {} unchanged",
                    r.added.len(),
                    r.removed.len(),
                    r.changed.len(),
                    r.unchanged
                );
            }
            return Ok(if r.ok { 0 } else { 1 });
        }
        Cmd::Schema => print!("{}", docs::SCHEMA),
        Cmd::Agents => {
            let files = scaffold::write_agents(&cli.dir)?;
            created(j, &cli.dir, files);
        }
        Cmd::Templates => {
            if j {
                let list: Vec<_> = new::TEMPLATES
                    .iter()
                    .map(|t| json!({"slug": t.slug, "about": t.about, "repo": format!("https://github.com/{}", t.repo)}))
                    .collect();
                print_json(&json!({"ok": true, "templates": list}));
            } else {
                for t in new::TEMPLATES {
                    println!(
                        "  {:<8} {:<26} https://github.com/{}",
                        t.slug, t.about, t.repo
                    );
                }
                println!(
                    "\n  Any GitHub repo, local folder or .zip works too: forno new my-site --template owner/repo"
                );
            }
        }
    }
    Ok(0)
}

fn main() {
    let cli = Cli::parse_from(legacy_args());
    let json_mode = cli.json;
    diag::set_quiet(json_mode);
    let code = match run(cli) {
        Ok(c) => c,
        Err(e) => {
            let errors: Vec<diag::Diagnostic> = match e.downcast_ref::<build::BuildError>() {
                Some(be) => be.errors.clone(),
                None => vec![diag::parse(&format!("{e:#}"), "error")],
            };
            if json_mode {
                print_json(
                    &json!({"ok": false, "errors": errors, "warnings": diag::take_warnings()}),
                );
            } else {
                eprintln!("error: {e:#}");
                if let Some(code) = errors.iter().find_map(|d| d.code) {
                    eprintln!("  (run `forno explain {code}` for help)");
                }
            }
            1
        }
    };
    std::process::exit(code);
}

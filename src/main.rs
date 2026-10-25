use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use static_rs::{build, dev, new};

#[derive(Parser)]
#[command(
    name = "static",
    version,
    about = "⚡ Static — the pure power of simplicity. A fast static site generator."
)]
struct Cli {
    /// Project directory (defaults to the current directory)
    #[arg(short = 'C', long = "dir", global = true, default_value = ".")]
    dir: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new site from a template
    New {
        /// Folder to create
        folder: String,
        /// Template: starter, aria, boxed, blog, stone, owner/repo, or a GitHub URL
        #[arg(short, long, default_value = "starter")]
        template: String,
        /// Don't run `npm install` afterwards
        #[arg(long)]
        no_install: bool,
        /// Start the dev server (and open the browser) when done
        #[arg(long)]
        dev: bool,
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
        /// Base URL for `{ url('/path') }` and sitemap.xml (default: relative)
        url: Option<String>,
        /// Output directory
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Don't run Tailwind or esbuild
        #[arg(long)]
        skip_tools: bool,
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
    /// List the official templates
    Templates,
}

/// Accept the original CLI's `static new my-site --blog` form.
fn legacy_args() -> Vec<String> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("new") {
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

fn run() -> Result<()> {
    let cli = Cli::parse_from(legacy_args());
    match cli.cmd {
        Cmd::New {
            folder,
            template,
            no_install,
            dev,
        } => {
            let path = new::new_project(&folder, &template, !no_install)?;
            if dev {
                dev::dev(
                    &path,
                    dev::DevOptions {
                        url: None,
                        port: None,
                        host: "127.0.0.1".into(),
                        open: true,
                    },
                )?;
            }
        }
        Cmd::Dev {
            url,
            port,
            host,
            open,
        } => dev::dev(
            &cli.dir,
            dev::DevOptions {
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
        } => {
            build::build(
                &cli.dir,
                build::BuildOptions {
                    url,
                    out,
                    skip_tools,
                },
            )?;
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
                    let site =
                        static_rs::site::Site::load(&cli.dir, static_rs::site::Mode::Build, None)?;
                    build::output_dir(&site, None)
                }
            };
            dev::serve(&dir, &host, port, open)?;
        }
        Cmd::Templates => {
            for t in new::TEMPLATES {
                println!(
                    "  {:<8} {:<26} https://github.com/{}",
                    t.slug, t.about, t.repo
                );
            }
            println!("\n  Use any GitHub repo too: static new my-site --template owner/repo");
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

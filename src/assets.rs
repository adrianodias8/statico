//! Asset pipeline: copy `public/` and `assets/`, compile Tailwind, bundle JS.
//!
//! Static itself is a single binary with no Node.js requirement. Tailwind and
//! esbuild are external tools; we look for them in this order:
//!   1. an explicit override (`STATIC_TAILWIND` / `STATIC_ESBUILD` env vars),
//!   2. the project's `node_modules/.bin`,
//!   3. a standalone binary on `PATH`,
//!   4. `npx --yes <package>`.
//!
//! If none are available the build still succeeds: JS is copied unbundled and
//! pages fall back to the in-browser Tailwind build, with a clear warning.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::site::{Site, Tailwind};

pub fn which(bin: &str) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            exts.iter()
                .map(|e| dir.join(format!("{bin}{e}")))
                .find(|p| p.is_file())
        })
    })
}

fn local_bin(root: &Path, bin: &str) -> Option<PathBuf> {
    let base = root.join("node_modules/.bin");
    let names = if cfg!(windows) {
        vec![format!("{bin}.cmd"), bin.to_string()]
    } else {
        vec![bin.to_string()]
    };
    names
        .into_iter()
        .map(|n| base.join(n))
        .find(|p| p.is_file())
}

fn npx() -> Option<PathBuf> {
    which("npx")
}

/// A resolved external command: program + leading args.
#[derive(Debug, Clone)]
pub struct Tool {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub label: String,
}

impl Tool {
    fn new(program: PathBuf, args: &[&str], label: impl Into<String>) -> Self {
        Tool {
            program,
            args: args.iter().map(|s| s.to_string()).collect(),
            label: label.into(),
        }
    }

    pub fn command(&self, root: &Path) -> Command {
        let mut c = Command::new(&self.program);
        c.args(&self.args).current_dir(root);
        c
    }
}

fn from_env(var: &str) -> Option<Tool> {
    let v = std::env::var(var).ok()?;
    let mut parts = v.split_whitespace();
    let program = PathBuf::from(parts.next()?);
    Some(Tool {
        program,
        args: parts.map(String::from).collect(),
        label: format!("${var}"),
    })
}

fn package_major(root: &Path, pkg: &str) -> Option<u64> {
    let text =
        std::fs::read_to_string(root.join("node_modules").join(pkg).join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["version"].as_str()?.split('.').next()?.parse().ok()
}

pub fn find_tailwind(root: &Path, kind: Tailwind) -> Option<Tool> {
    if let Some(t) = from_env("STATIC_TAILWIND") {
        return Some(t);
    }
    match kind {
        Tailwind::None => None,
        Tailwind::V3 => {
            if package_major(root, "tailwindcss") == Some(3)
                && let Some(p) = local_bin(root, "tailwindcss")
            {
                return Some(Tool::new(p, &[], "node_modules/.bin/tailwindcss (v3)"));
            }
            npx().map(|p| Tool::new(p, &["--yes", "tailwindcss@3"], "npx tailwindcss@3"))
        }
        Tailwind::V4 => {
            if root.join("node_modules/@tailwindcss/cli").is_dir()
                && let Some(p) = local_bin(root, "tailwindcss")
            {
                return Some(Tool::new(p, &[], "node_modules/.bin/tailwindcss (v4)"));
            }
            if let Some(p) = which("tailwindcss") {
                return Some(Tool::new(p, &[], "tailwindcss (standalone)"));
            }
            npx().map(|p| {
                Tool::new(
                    p,
                    &["--yes", "@tailwindcss/cli@4"],
                    "npx @tailwindcss/cli@4",
                )
            })
        }
    }
}

pub fn find_esbuild(root: &Path) -> Option<Tool> {
    if let Some(t) = from_env("STATIC_ESBUILD") {
        return Some(t);
    }
    if let Some(p) = local_bin(root, "esbuild") {
        return Some(Tool::new(p, &[], "node_modules/.bin/esbuild"));
    }
    if let Some(p) = which("esbuild") {
        return Some(Tool::new(p, &[], "esbuild"));
    }
    npx().map(|p| Tool::new(p, &["--yes", "esbuild"], "npx esbuild"))
}

fn run(tool: &Tool, root: &Path, extra: &[String]) -> Result<()> {
    let out = tool
        .command(root)
        .args(extra)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to start {}", tool.label))?;
    if !out.status.success() {
        bail!(
            "{} failed ({}):\n{}{}",
            tool.label,
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Compile `assets/css/main.css` → `<out>/assets/css/main.css`.
/// Returns Ok(false) if no compiler was available (caller falls back to CDN).
pub fn build_css(site: &Site, out_dir: &Path) -> Result<bool> {
    let input = site.root.join("assets/css/main.css");
    if !input.is_file() {
        return Ok(false);
    }
    let Some(tool) = find_tailwind(&site.root, site.tailwind) else {
        eprintln!(
            "warning: no Tailwind compiler found (install Node.js, or put the standalone `tailwindcss` binary on PATH).\n         Pages will use the in-browser Tailwind build instead."
        );
        return Ok(false);
    };
    let output = out_dir.join("assets/css/main.css");
    std::fs::create_dir_all(output.parent().unwrap())?;
    let mut args = vec![
        "-i".to_string(),
        input.display().to_string(),
        "-o".to_string(),
        output.display().to_string(),
        "--minify".to_string(),
    ];
    if site.tailwind == Tailwind::V3 && site.root.join("tailwind.config.js").is_file() {
        args.push("-c".into());
        args.push(site.root.join("tailwind.config.js").display().to_string());
    }
    run(&tool, &site.root, &args).context("compiling Tailwind CSS")?;
    Ok(true)
}

/// Bundle `assets/js/main.js` → `<out>/assets/js/main.js` (minified).
pub fn build_js(site: &Site, out_dir: &Path) -> Result<()> {
    let input = site.root.join("assets/js/main.js");
    if !input.is_file() {
        return Ok(());
    }
    let Some(tool) = find_esbuild(&site.root) else {
        eprintln!("warning: esbuild not found; assets/js/main.js was copied without bundling.");
        return Ok(());
    };
    let output = out_dir.join("assets/js/main.js");
    run(
        &tool,
        &site.root,
        &[
            input.display().to_string(),
            "--bundle".into(),
            "--minify".into(),
            format!("--outfile={}", output.display()),
            "--log-level=warning".into(),
        ],
    )
    .context("bundling JavaScript")
}

/// Recursively copy `src` into `dst`, skipping dotfiles like `.gitkeep`/`.DS_Store`.
pub fn copy_dir(src: &Path, dst: &Path) -> Result<usize> {
    if !src.is_dir() {
        return Ok(0);
    }
    let mut n = 0;
    for e in walkdir::WalkDir::new(src) {
        let e = e?;
        let rel = e.path().strip_prefix(src)?;
        if rel
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let target = dst.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if e.file_type().is_file() {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(e.path(), &target).with_context(|| {
                format!("copying {} to {}", e.path().display(), target.display())
            })?;
            n += 1;
        }
    }
    Ok(n)
}

/// If `package.json` exists but `node_modules` doesn't, offer to install
/// (Tailwind v3 configs usually `require()` plugins).
pub fn ensure_node_modules(root: &Path) -> Result<()> {
    if !root.join("package.json").is_file() || root.join("node_modules").is_dir() {
        return Ok(());
    }
    if detect_requires(root)
        && let Some(npm) = which("npm")
    {
        eprintln!("Installing npm dependencies (tailwind.config.js requires plugins)…");
        let status = Command::new(npm)
            .arg("install")
            .arg("--no-fund")
            .arg("--no-audit")
            .current_dir(root)
            .status()?;
        if !status.success() {
            bail!("npm install failed");
        }
    }
    Ok(())
}

fn detect_requires(root: &Path) -> bool {
    std::fs::read_to_string(root.join("tailwind.config.js")).is_ok_and(|s| s.contains("require("))
}

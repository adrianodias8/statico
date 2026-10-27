//! Managed tool downloads, so building a Tailwind site never requires Node.js.
//!
//! * Tailwind CSS: the official standalone CLI from GitHub releases (it bundles
//!   the first-party plugins such as typography and forms).
//! * esbuild: the platform binary from the npm registry tarball.
//!
//! Binaries are cached in `~/.cache/statico/tools/` (or `%LOCALAPPDATA%\statico`)
//! and downloaded once. Set `STATICO_OFFLINE=1` to disable downloads.

use anyhow::{Context, Result, bail};
use std::io::Read;
use std::path::PathBuf;

pub const TAILWIND_V3: &str = "3.4.19";
pub const TAILWIND_V4: &str = "4.3.3";
pub const ESBUILD: &str = "0.28.2";

pub fn cache_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("STATICO_CACHE_DIR") {
        return Some(PathBuf::from(d));
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    }?;
    Some(base.join("statico").join("tools"))
}

fn offline() -> bool {
    std::env::var("STATICO_OFFLINE").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// `(os, arch)` names used by Tailwind release assets.
fn tailwind_asset() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "tailwindcss-linux-x64",
        ("linux", "aarch64") => "tailwindcss-linux-arm64",
        ("macos", "x86_64") => "tailwindcss-macos-x64",
        ("macos", "aarch64") => "tailwindcss-macos-arm64",
        ("windows", "x86_64") => "tailwindcss-windows-x64.exe",
        _ => return None,
    })
}

fn esbuild_platform() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("macos", "aarch64") => "darwin-arm64",
        ("windows", "x86_64") => "win32-x64",
        ("windows", "aarch64") => "win32-arm64",
        _ => return None,
    })
}

fn http_get(url: &str) -> Result<Vec<u8>> {
    use ureq::tls::{RootCerts, TlsConfig};
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    let mut resp = agent
        .get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let mut buf = Vec::new();
    resp.body_mut()
        .with_config()
        .limit(300 * 1024 * 1024)
        .reader()
        .read_to_end(&mut buf)?;
    Ok(buf)
}

fn install(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    // Write to a temp name first so a half-finished download is never used.
    let tmp = path.with_extension("partial");
    std::fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Path to a cached standalone Tailwind CLI, downloading it if needed.
pub fn tailwind(major: u8) -> Option<PathBuf> {
    let version = if major == 3 { TAILWIND_V3 } else { TAILWIND_V4 };
    let asset = tailwind_asset()?;
    let path = cache_dir()?.join(format!(
        "tailwindcss-{version}{}",
        if cfg!(windows) { ".exe" } else { "" }
    ));
    if path.is_file() {
        return Some(path);
    }
    if offline() {
        return None;
    }
    crate::say!("  Downloading Tailwind CSS {version} standalone (one-time)…");
    let url =
        format!("https://github.com/tailwindlabs/tailwindcss/releases/download/v{version}/{asset}");
    match http_get(&url).and_then(|b| install(&path, &b)) {
        Ok(()) => Some(path),
        Err(e) => {
            crate::diag::warn(format!("could not download Tailwind: {e:#}"));
            None
        }
    }
}

/// Path to a cached esbuild binary, downloading it if needed.
pub fn esbuild() -> Option<PathBuf> {
    let platform = esbuild_platform()?;
    let exe = if cfg!(windows) {
        "esbuild.exe"
    } else {
        "esbuild"
    };
    let path = cache_dir()?.join(format!(
        "esbuild-{ESBUILD}{}",
        if cfg!(windows) { ".exe" } else { "" }
    ));
    if path.is_file() {
        return Some(path);
    }
    if offline() {
        return None;
    }
    crate::say!("  Downloading esbuild {ESBUILD} (one-time)…");
    let url = format!("https://registry.npmjs.org/@esbuild/{platform}/-/{platform}-{ESBUILD}.tgz");
    let res = http_get(&url).and_then(|tgz| {
        let want = if cfg!(windows) {
            format!("package/{exe}")
        } else {
            format!("package/bin/{exe}")
        };
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&tgz[..]));
        for entry in archive.entries()? {
            let mut entry = entry?;
            if entry.path()?.to_string_lossy() == want {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)?;
                return install(&path, &bytes);
            }
        }
        bail!("{want} not found in {url}")
    });
    match res {
        Ok(()) => Some(path),
        Err(e) => {
            crate::diag::warn(format!("could not download esbuild: {e:#}"));
            None
        }
    }
}

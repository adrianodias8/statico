//! Managed tool downloads, so building a Tailwind site never requires Node.js.
//!
//! * Tailwind CSS: the official standalone CLI from GitHub releases (it bundles
//!   the first-party plugins such as typography and forms).
//! * esbuild: the platform binary from the npm registry tarball.
//!
//! Every download is checked against a SHA-256 pinned below before it is written
//! to disk, so a tampered release or registry can't get a binary executed.
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

/// Tailwind release asset for each `(os, arch)`.
const TAILWIND_ASSETS: &[(&str, &str, &str)] = &[
    ("linux", "x86_64", "tailwindcss-linux-x64"),
    ("linux", "aarch64", "tailwindcss-linux-arm64"),
    ("macos", "x86_64", "tailwindcss-macos-x64"),
    ("macos", "aarch64", "tailwindcss-macos-arm64"),
    ("windows", "x86_64", "tailwindcss-windows-x64.exe"),
];

/// esbuild npm platform package for each `(os, arch)`.
const ESBUILD_PLATFORMS: &[(&str, &str, &str)] = &[
    ("linux", "x86_64", "linux-x64"),
    ("linux", "aarch64", "linux-arm64"),
    ("macos", "x86_64", "darwin-x64"),
    ("macos", "aarch64", "darwin-arm64"),
    ("windows", "x86_64", "win32-x64"),
    ("windows", "aarch64", "win32-arm64"),
];

/// SHA-256 of every file Statico downloads. Tailwind's are from each release's
/// own `sha256sums.txt`; esbuild's are of the npm tarball, cross-checked against
/// the registry's `dist.integrity`. Bumping a version means adding its hashes
/// here, and a test fails until every platform has one.
const CHECKSUMS: &[(&str, &str)] = &[
    (
        "tailwindcss@3.4.19/tailwindcss-linux-x64",
        "4af3198c015616ea7d6617974ec3d70d987ecc00c1ca8463b0a30fd65cc7c06e",
    ),
    (
        "tailwindcss@3.4.19/tailwindcss-linux-arm64",
        "e5b2d27694daa80cc52ec29553ba2c6bd43d86bd51a9d633ed24058b9c05a676",
    ),
    (
        "tailwindcss@3.4.19/tailwindcss-macos-x64",
        "a597f407e0f1f03535731f5b42f1576a8152cb5fffc2f38e754722bc0c280045",
    ),
    (
        "tailwindcss@3.4.19/tailwindcss-macos-arm64",
        "7fdeb00818b6214a337383063282b2361ecb08bbc08f8c8a7ba97ee1e2eaa4fe",
    ),
    (
        "tailwindcss@3.4.19/tailwindcss-windows-x64.exe",
        "a15158c4c5e0e7a75f7229bfe4986fe7710d2edc468b6f96c8981f78ab211347",
    ),
    (
        "tailwindcss@4.3.3/tailwindcss-linux-x64",
        "dc61b3ac6b8c9ca874c0cc4c57b2409791a64c5540404ca5f5367360babc313a",
    ),
    (
        "tailwindcss@4.3.3/tailwindcss-linux-arm64",
        "55fd0b241214eff3de1e8ee4f22796662f2d2e7a49bcfca7477cfd0bac398195",
    ),
    (
        "tailwindcss@4.3.3/tailwindcss-macos-x64",
        "7922e0953f2110c05976e3bf58f14e643d90427575e766b7d433f5f80cbee7e1",
    ),
    (
        "tailwindcss@4.3.3/tailwindcss-macos-arm64",
        "cdf646702987a743464dff4d9c60fd4480d1c1e73dd819a9a67f1078815dce9d",
    ),
    (
        "tailwindcss@4.3.3/tailwindcss-windows-x64.exe",
        "e0e260ce048014e9268f6237ff18f8ccf02cef521cbd0ae04e82c2cdf7aa3955",
    ),
    (
        "esbuild@0.28.2/linux-x64",
        "9573bb2233aab0f9ea7647d5cca9726113cc1768de61d66b17267f4db84488f6",
    ),
    (
        "esbuild@0.28.2/linux-arm64",
        "a96dbfa41d3ef5dbd1ef22b1c10d5187be9267e86093a870f06402a7ec931596",
    ),
    (
        "esbuild@0.28.2/darwin-x64",
        "abb6a7a895aaf2cc0df36fecc3bf0042479ffec51a643a056d54e9109f27db55",
    ),
    (
        "esbuild@0.28.2/darwin-arm64",
        "1980cde09749094452b20d36ff267585ccb3f72749c7bc97291cd9996ccf5a2a",
    ),
    (
        "esbuild@0.28.2/win32-x64",
        "7286c3611b6f1f4c4d9ec90adcbc478407ff0d28ead96567f361d53e67613e19",
    ),
    (
        "esbuild@0.28.2/win32-arm64",
        "b3cffeaa165134f7da679c8619266953afd5f41641a254615a57e757f67c7336",
    ),
];

fn for_this_platform(table: &[(&str, &str, &'static str)]) -> Option<&'static str> {
    table
        .iter()
        .find(|(os, arch, _)| *os == std::env::consts::OS && *arch == std::env::consts::ARCH)
        .map(|(_, _, name)| *name)
}

fn checksum(key: &str) -> Option<&'static str> {
    CHECKSUMS.iter().find(|(k, _)| *k == key).map(|(_, h)| *h)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, bytes);
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn verify(key: &str, want: &str, bytes: &[u8]) -> Result<()> {
    let got = sha256_hex(bytes);
    if got != want {
        bail!(
            "checksum mismatch for {key}: expected {want}, got {got}. \
             The download was discarded; nothing was run"
        );
    }
    Ok(())
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
    let asset = for_this_platform(TAILWIND_ASSETS)?;
    let key = format!("tailwindcss@{version}/{asset}");
    let Some(want) = checksum(&key) else {
        crate::diag::warn(format!("no pinned checksum for {key}; not downloading it"));
        return None;
    };
    // The hash is part of the name, so a binary cached before checksums
    // existed is never picked up unverified.
    let path = cache_dir()?.join(format!(
        "tailwindcss-{version}-{}{}",
        &want[..12],
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
    match http_get(&url).and_then(|b| {
        verify(&key, want, &b)?;
        install(&path, &b)
    }) {
        Ok(()) => Some(path),
        Err(e) => {
            crate::diag::warn(format!("could not download Tailwind: {e:#}"));
            None
        }
    }
}

/// Path to a cached esbuild binary, downloading it if needed.
pub fn esbuild() -> Option<PathBuf> {
    let platform = for_this_platform(ESBUILD_PLATFORMS)?;
    let key = format!("esbuild@{ESBUILD}/{platform}");
    let Some(want) = checksum(&key) else {
        crate::diag::warn(format!("no pinned checksum for {key}; not downloading it"));
        return None;
    };
    let exe = if cfg!(windows) {
        "esbuild.exe"
    } else {
        "esbuild"
    };
    let path = cache_dir()?.join(format!(
        "esbuild-{ESBUILD}-{}{}",
        &want[..12],
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
        verify(&key, want, &tgz)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_download_has_a_checksum() {
        for version in [TAILWIND_V3, TAILWIND_V4] {
            for (_, _, asset) in TAILWIND_ASSETS {
                let key = format!("tailwindcss@{version}/{asset}");
                assert!(checksum(&key).is_some(), "missing checksum for {key}");
            }
        }
        for (_, _, platform) in ESBUILD_PLATFORMS {
            let key = format!("esbuild@{ESBUILD}/{platform}");
            assert!(checksum(&key).is_some(), "missing checksum for {key}");
        }
        for (key, hash) in CHECKSUMS {
            assert!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "{key}: not a SHA-256"
            );
        }
    }

    #[test]
    fn verify_rejects_a_tampered_download() {
        let want = sha256_hex(b"tailwind");
        assert!(verify("t", &want, b"tailwind").is_ok());
        let err = verify("t", &want, b"tai1wind").unwrap_err().to_string();
        assert!(err.contains("checksum mismatch"), "{err}");
    }
}

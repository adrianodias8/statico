---
title: Install
description: One binary for macOS, Linux and Windows; the install script, release downloads, or a build from source.
section: start
order: 2
---

## macOS and Linux

```bash
curl -fsSL https://raw.githubusercontent.com/adrianodias8/statico/main/packaging/install.sh | sh
```

The script picks the build for your system and puts a `statico` binary in `~/.local/bin`. Set `STATICO_INSTALL_DIR` to install somewhere else.

## Downloads

Every [release](https://github.com/adrianodias8/statico/releases) has builds for:

| System | File |
|---|---|
| macOS, Apple Silicon | `statico-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `statico-x86_64-apple-darwin.tar.gz` |
| Linux, x86-64 | `statico-x86_64-unknown-linux-musl.tar.gz` |
| Linux, ARM64 | `statico-aarch64-unknown-linux-musl.tar.gz` |
| Windows, x86-64 | `statico-x86_64-pc-windows-msvc.zip` |

The Linux builds are static, so they run on any distribution. The Intel Mac build is published from v0.2.0 on.

## From source

With a Rust toolchain:

```bash
git clone https://github.com/adrianodias8/statico
cd statico
cargo build --release
```

The binary is at `target/release/statico`.

## Check it works

```bash
statico --version
statico docs          # this reference, offline
```

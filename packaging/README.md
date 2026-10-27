# Packaging

Release flow: push a tag `vX.Y.Z` → `.github/workflows/ci.yml` builds static
binaries for Linux (musl, x64/arm64), macOS (x64/arm64) and Windows, and
attaches them to the GitHub release as `statico-<target>.tar.gz|zip`.

The release repository is `adrianodias8/statico`, hard-coded in `install.sh`
(curl | sh installer) and `npm/install.js`; both honour `STATICO_REPO` as an
override. `homebrew/statico.rb` still needs the `sha256` values filled in from
the release artifacts, and a tap to live in (e.g. `adrianodias8/homebrew-tap`).

## Names (checked Oct 2026)

- **crates.io:** `statico` is taken (an unrelated crate), so the package is
  published as **`statico-cli`** (`cargo install statico-cli`). The binary and
  library are still called `statico`.
- **npm:** unscoped `statico` is taken too (a 2016 package), so the wrapper is
  published as **`statico-cli`** (`npm i -g statico-cli`), installing a
  `statico` binary. Publish with `cd npm && npm publish`.
- **Homebrew:** free; the formula installs the binary as `statico`.

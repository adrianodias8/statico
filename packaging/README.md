# Packaging

Release flow: push a tag `vX.Y.Z` → `.github/workflows/ci.yml` builds static
binaries for Linux (musl, x64/arm64), macOS (x64/arm64) and Windows, and
attaches them to the GitHub release as `forno-<target>.tar.gz|zip`.

Before the first release, replace `OWNER/forno` with the real repository in:

- `install.sh` (curl | sh installer) — or set `FORNO_REPO`
- `npm/install.js` — or set `FORNO_REPO`; publish with `cd npm && npm publish`
  (the unscoped `forno` name was free on npm in Oct 2026)
- `homebrew/forno.rb` — also fill in the `sha256` values from the release

crates.io: the crate name `forno` is taken (an empty placeholder), so the
package is published as **`forno-cli`** (`cargo install forno-cli`); the
binary and library are still called `forno`. Consider asking the owner of the
`forno` crate to transfer it.

# Releasing Sahou

Each published artifact is released **independently** by pushing a **prefixed git
tag**. Only the workflow matching that tag prefix runs, so a fix to one channel never
rebuilds or republishes the others.

## Channels

| Push tag        | Workflow                              | Publishes to                                       | Version source                       |
| --------------- | ------------------------------------- | -------------------------------------------------- | ------------------------------------ |
| `v0.0.3`        | `release.yml` (cargo-dist) + `crates-release.yml` | GitHub Release + shell/powershell installers, **and** crates.io (`sahou-core` → `sahou-cli`) | `core/Cargo.toml`, `cli/Cargo.toml`  |
| `py-v0.0.3`     | `python-release.yml`                  | PyPI (`sahou`)                                     | `runtimes/python/pyproject.toml`     |
| `npm-v0.0.3`    | `npm-release.yml`                     | npm (`sahou`)                                      | `runtimes/typescript/package.json`   |

The tag only *triggers* the release; the version that gets published comes from the
manifest, not the tag. Keep them equal to avoid confusion.

`release.yml`'s trigger is intentionally narrowed to `v[0-9]+.…` so runtime tags
(`py-v*`, `npm-v*`) do not cross-fire it. This edit is preserved across `dist generate`
by `allow-dirty = ["ci"]` in `dist-workspace.toml` — keep both in sync.

## Version policy

- Versions are **per channel** (npm can be at `0.0.5` while the CLI is at `0.0.2`).
  Only bump and re-tag the channel(s) you actually changed.
- Align all channels on the same number only for a deliberate milestone
  (a coordinated minor/major).
- The Rust core (`core/`) is **shared**: it is the crates.io `sahou-core`, is compiled
  to wasm for the npm package, and is built via pyo3 for the PyPI package. A change to
  `core/` therefore usually warrants re-releasing all three channels; a change confined
  to one runtime's glue only needs that channel.

## Prerequisites (one-time, already configured)

- Repo secret `CARGO_REGISTRY_TOKEN` — crates.io token with publish rights for both crates.
- Repo secret `NPM_TOKEN` — npm automation/granular token with publish rights for `sahou`.
- PyPI uses **Trusted Publishing (OIDC)** — no secret needed (configured on PyPI).

## How to release

### CLI + crates.io (`v*`)

1. Bump `version` in `core/Cargo.toml` and `cli/Cargo.toml`, and the `sahou-core`
   dependency `version` in `cli/Cargo.toml` to match.
2. Rebuild the embedded GUI so the CLI binary and the crate carry the current core:
   ```sh
   just gui-build && just gui-embed   # builds gui/dist, copies into cli/gui-dist/
   ```
   (`cli/gui-dist/` is committed and embedded via rust-embed; both the cargo-dist
   binaries and the crates.io package consume it, so it must be fresh.)
3. Refresh the lockfile: `cargo update -p sahou-core -p sahou-cli`.
4. Commit everything to `main` and push.
5. Tag and push:
   ```sh
   git tag -a v0.0.3 -m "sahou v0.0.3 (CLI + crates.io)"
   git push origin v0.0.3
   ```
   This runs `release.yml` (GitHub Release + installers) and `crates-release.yml`
   (`sahou-core` then `sahou-cli`).

### PyPI (`py-v*`)

1. Bump `version` in `runtimes/python/pyproject.toml`.
2. Commit to `main` and push.
3. Tag and push:
   ```sh
   git tag -a py-v0.0.3 -m "sahou Python runtime v0.0.3"
   git push origin py-v0.0.3
   ```
   (The workflow also has a manual `workflow_dispatch` fallback in the Actions UI.)

### npm (`npm-v*`)

1. Bump `version` in `runtimes/typescript/package.json`.
2. Commit to `main` and push.
3. Tag and push:
   ```sh
   git tag -a npm-v0.0.3 -m "sahou TypeScript runtime v0.0.3"
   git push origin npm-v0.0.3
   ```
   The workflow builds the wasm core (node + web) and the tsc output, then publishes
   with provenance.

### Coordinated "release everything" (milestone)

Bump all four manifests to the same version, do the CLI GUI rebuild step, commit, then
push all three tags together:
```sh
git push origin v0.1.0 py-v0.1.0 npm-v0.1.0
```

## After pushing

Watch the runs: `gh run list` (or the Actions tab). Publishing can fail on token
issues, an already-published version, or the crates.io index race — check the failing
job's log. crates.io / npm / PyPI cannot be re-published at the same version, so a
failed publish means bumping the patch and re-tagging.

## Notes

- cargo-dist has **no** built-in crates.io publisher — that is why `crates-release.yml`
  exists as a separate workflow rather than a cargo-dist `publish-jobs` entry.
- Release build speed: `release.yml`'s per-target build job caches with
  `Swatinem/rust-cache`, and `[profile.dist]` in the root `Cargo.toml` sets `lto = false`
  (zenoh is large; LTO dominated link time). Build targets are configured in
  `dist-workspace.toml` (`targets`); Apple Silicon + Linux (x86_64/aarch64) + Windows,
  Intel macOS dropped.

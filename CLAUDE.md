# Kojin — Claude Code Instructions

## Project Structure

Rust workspace with multiple crates. All crates use `version.workspace = true` from root `Cargo.toml`.

## Checklist: Adding a New Crate

When adding a new crate to the workspace:

1. Add to `[workspace] members` in root `Cargo.toml`
2. Add inter-crate dependency with `path` + `version` in consuming crates
3. **Update `.github/workflows/publish.yml`** — add the new crate in dependency order (leaf crates before dependents)
4. If the umbrella `kojin` crate re-exports it, add an optional dependency + feature flag in `kojin/Cargo.toml`

## Checklist: Releasing a New Version

1. Bump `version` in root `Cargo.toml` (workspace-level)
2. Bump all inter-crate `version = "x.y.z"` references
3. Update `README.md` version references
4. Run `cargo fmt --all` before committing
5. Run full verification: `cargo build/test/clippy --workspace --all-features`
6. Commit `Cargo.lock` together with version bumps
7. Push commit first, then tag + push tag (avoids CI race condition)

## CI

- `ci.yml` — runs on every push: fmt, clippy, tests
- `publish.yml` — runs on `v*` tags: publishes crates to crates.io in dependency order

# pixpixpix

## Agent skills

### Issue tracker

Issues live in GitHub Issues of `paxel/pixpixpix`, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Rules

* `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` before every commit, on the toolchain `rust-toolchain.toml` pins.
* Shipped code may not panic: `unwrap`, `expect`, `panic`, `todo`, `unimplemented` and `dbg!` are denied in `Cargo.toml`, `unreachable`, indexing, slicing and overflowing arithmetic in `src/lib.rs`. Tests are exempt. No `#[allow]`: fix the lint.
* Unsafe is denied crate-wide. Only the FFI modules (`mpv`, `heif`, `turbojpeg`, `macos`) opt in with `#[expect(unsafe_code, reason = "…")]`, and every unsafe block carries a `// SAFETY:` comment.
* Coverage ratchet: the Rust gate in `ci.yml` fails under the floor in `coverage-floors.toml`. When coverage rises, raise the floor and the badge in the same pull request, on a machine with every native library installed, with gates checked out beside the repository: `cargo tarpaulin --engine llvm --locked --all-features --post-test-delay 2 --skip-clean --out Lcov --output-dir coverage && mv coverage/lcov.info coverage/rust.lcov`, then `python3 ../gates/scripts/coverage_ratchet.py --floors coverage-floors.toml`. Never lower it.
* A dependency advisory is fixed by an update; an `ignore` in `deny.toml` only with its reason, when no fixed release exists.
* `summer` takes pull requests only; the required checks are the CI lanes, the coverage ratchet, cargo deny and CodeQL.
* Commits: one-line summary, `[pix-<issue>]` first for ticket work, no trailers.

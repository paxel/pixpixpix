# pixpixpix

## Agent skills

### Issue tracker

Issues live in GitHub Issues of `paxel/pixpixpix`, via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## Rules

* The standard pixpixpix is held to is [paxel/gates](https://github.com/paxel/gates); `docs/adr/0001-move-to-gates.md` says where pixpixpix adds to it and why. The workflows call gates' shared workflows, pinned by commit; Dependabot moves the pin.
* `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test` before every commit, on the toolchain `rust-toolchain.toml` pins. Clippy on the tests is a second run with the lints allowed that are how a test fails: `cargo clippy --tests --benches --examples -- -D warnings -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic -A clippy::indexing_slicing -A clippy::string_slice -A clippy::arithmetic_side_effects -A clippy::unreachable`.
* Shipped code may not panic: `unwrap`, `expect`, `panic`, `todo`, `unimplemented`, `unreachable`, indexing, slicing, overflowing arithmetic and `dbg!` are denied in `Cargo.toml`, for the library and both binaries. No `#[allow]`: fix the lint.
* Unsafe is denied crate-wide. Only the FFI modules (`mpv`, `heif`, `turbojpeg`, `macos`) opt in with `#[expect(unsafe_code, reason = "…")]`, and every unsafe block carries a `// SAFETY:` comment.
* Coverage ratchet: the Rust gate in `ci.yml` fails under the floor in `coverage-floors.toml`. When coverage rises, raise the floor and the badge in the same pull request, on a machine with every native library installed, with gates checked out beside the repository: `cargo tarpaulin --engine llvm --locked --all-features --post-test-delay 2 --skip-clean --out Lcov --output-dir coverage && mv coverage/lcov.info coverage/rust.lcov`, then `python3 ../gates/scripts/coverage_ratchet.py --floors coverage-floors.toml`. Never lower it.
* A dependency advisory is fixed by an update. When no fixed release exists, it is accepted in `osv-scanner.toml` (and in `fuzz/osv-scanner.toml` for the fuzz crate's lockfile) with its reason and an `ignoreUntil` date; never without both.
* A code-scanning alert or a SonarQube Cloud finding that stays open is accepted in `code-scanning-ignores.toml` with tool, rule, file, reason and an `ignoreUntil` date. Nothing is dismissed in GitHub or marked in SonarQube Cloud.
* `summer` takes pull requests only; the required checks are the shared Rust gate, the OSV gate, SonarQube Cloud, the lanes for Linux aarch64 and macOS arm64, and CodeQL.
* Commits: one-line summary, `[pix-<issue>]` first for ticket work, no trailers.

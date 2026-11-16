# 1. pixpixpix moves to the gates standard

Date: 2026-11-16

## Status

Accepted.

## Context

pixpixpix had hardened itself in its own way ([pix-95](https://github.com/paxel/pixpixpix/issues/95)
to [pix-111](https://github.com/paxel/pixpixpix/issues/111)): its own CI lanes, a coverage
ratchet script, cargo deny, its own release gate and provenance step. Every paxel repository
had done so, each differently. [paxel/gates](https://github.com/paxel/gates) is the one
standard that replaces those, as shared workflows a repository calls, and its weekly audit
found 16 checks of it missing here. gates 1.1.0 also counts every open SonarQube Cloud
finding, of which pixpixpix had 17.

## Decision

pixpixpix meets the standard of gates v1.1.0. Where the standard decides, it is followed
without a note here: the OSV gate instead of cargo deny, one coverage ratchet, the shared
CodeQL, Scorecard, fuzz, release gate and provenance workflows, the files and the badge row.
What follows is where pixpixpix adds to it, or had a choice.

**Test lanes.** The shared Rust gate runs format, the lint set, clippy, the tests and the
coverage ratchet on Linux x86_64. Its other runners only build and run clippy, and it
installs no Homebrew packages, so a macOS build of pixpixpix would not link there. pixpixpix
has FFI code per platform and ships for Linux aarch64 and macOS arm64, so `ci.yml` keeps two
lanes of its own for them, with the native libraries, format, clippy and every test.

**Coverage.** `scripts/coverage.sh` and `.coverage-floor` are gone. The floor is in
`coverage-floors.toml`; raising it locally is two commands, named in `AGENTS.md`.

**Release.** The shared release gate checks alerts, findings, vulnerabilities, licences and
the changelog section, but runs no tests. A tag can sit on any commit, and a release feeds
the Homebrew tap and the Flatpak bundle, so the release keeps a job of its own that checks
the tag against the crate version and runs format, clippy and every test on the tagged
commit.

**AccessKit and its unmaintained crates.** RUSTSEC-2026-0370, -0371 and -0372 are
unmaintained notices for zvariant, zbus_names and zvariant_derive. They reach pixpixpix only
through eframe's `accesskit` feature (accesskit_unix on zbus 5), which is what makes the UI
visible to screen readers. No release of accesskit_unix on zbus 6 exists. The feature stays;
the notices are accepted until 2027-02-14, three months, because they wait for an upstream
release and are no scanner's misreading. Dropping the feature to clean the report was
rejected: it would take something from users to please a scanner.

**SonarQube Cloud findings.** Of the 17, twelve are gone: five with the coverage script,
four regular expressions rewritten, `Number.parseInt`, and the two `curl` calls of the
Flatpak installer, which now refuse anything but HTTPS. Five are accepted, per file:

- `tool/site/build-all.js`, three times "PATH": the site build starts `git` and `tar` by
  name. It is run by the Pages workflow and the maintainer; absolute paths would break it
  wherever those live elsewhere.
- `tool/site/build.js`, twice "filesystem oracle": the local preview server already refuses
  every path that leaves the output folder, and serves those files anyway.

**Scorecard.** "Vulnerabilities" lists the three notices above and is accepted with them.
"Maintained" fires for every repository younger than 90 days and is accepted until
2027-01-24, when it ends by itself.

**Required checks.** The ruleset on `summer` requires the shared Rust gate, the OSV gate,
SonarQube Cloud, the two own lanes and both CodeQL analyses. Dependabot's pull requests need
`SQ_TOKEN` among the Dependabot secrets for the SonarQube Cloud check.

## Consequences

- cargo deny's checks for yanked crates and for crates from unknown registries have no
  counterpart in the OSV gate and are lost.
- The four lints that only the library denied (`unreachable`, indexing, slicing, overflowing
  arithmetic) now hold for both binaries too.
- Fuzzing runs weekly and by hand, a quarter of an hour per target, no longer for a minute
  on every pull request.
- SonarQube Cloud analyses pull requests as well as `summer`, and no longer imports a clippy
  report: clippy fails CI on any warning, so the report was always empty.
- The release needs the secret `GATES_ALERTS_TOKEN`, a fine-grained token that can read
  secret-scanning alerts; without it the release gate and the weekly Security run fail.
- A release carries a CycloneDX SBOM of its source, and its Sigstore bundle is named
  `pixpixpix-<tag>-provenance.sigstore.json`.

# Security policy

## Reporting a vulnerability

Please report a vulnerability in pixpixpix (the binaries, the macOS app, the Flatpak
bundle, the install scripts and the Homebrew formula and cask) privately, through GitHub's
[private vulnerability reporting](https://github.com/paxel/pixpixpix/security/advisories/new)
(the repository's **Security** tab, **Report a vulnerability**). Do not open a public issue,
pull request or discussion for it.

Include what is affected (version, platform, file type), how to reproduce it (a sample file
helps most), and what an attacker gains, for example code execution from a crafted image or
video, or a file written or deleted outside what you asked for. The report and its fix are
discussed in that advisory, which stays private until a release with the fix is out.

## What is already checked

Every pull request and every change to the summer branch runs CodeQL, a dependency audit
(osv-scanner) and SonarQube Cloud; OpenSSF Scorecard analyses the summer branch, and the fuzz
targets of the hand-written file parsers run every week. The CodeQL and Scorecard findings
are the repository's code scanning alerts, SonarQube Cloud's are on its project page; a
report does not need to repeat them. A release is not built while one of them is open and
not accepted in `code-scanning-ignores.toml` with a reason and a date.

## Verifying a release

Every release file carries SLSA build provenance, signed with Sigstore by the release
workflow. `gh attestation verify <file> --repo paxel/pixpixpix` checks that a file was built
by this repository's workflow from the tagged commit.

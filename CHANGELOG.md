# Changelog

## [Unreleased]

## [2.0.3] - 2026-07-21

### Security
- Updated openssl 0.10.76 → 0.10.81, resolving 9 advisories (out-of-bounds writes/reads, undefined behavior, buffer overflows in rust-openssl)
- Updated rustls-webpki 0.103.10 → 0.103.13, resolving 3 advisories (DoS panic on malformed CRL, incorrect name-constraint acceptance)
- Bumped tray-icon 0.22.0 → 0.24.1 (and muda 0.17 → 0.19) to pick up dependency updates; explicitly enabled the `gtk` feature (without `libxdo`) to preserve the libxdo-free Linux build from a prior release

### Accepted risk
- `glib` remains on 0.18.5 (patched version is 0.20.0), medium-severity soundness bug in `VariantStrIter`. It's pulled in transitively through `tray-icon`'s only Linux backend (`libappindicator`, a GTK3 library) — `gtk`/gtk3-rs bindings top out at 0.18.2 upstream and there is no GTK4-based appindicator equivalent. Fixing this would mean dropping `tray-icon` on Linux for a hand-rolled D-Bus StatusNotifierItem tray implementation, a major undertaking disproportionate to a soundness bug in an Iterator impl this app never exercises (no untrusted GVariant data is parsed). Accepted as a known, permanent limitation rather than deferred.

## [2.0.2] - 2026-07-21

### Changed
- Bumped winreg 0.52.0 → 0.56.0
- Bumped eframe and egui 0.34.0 → 0.34.1
- Bumped notify-rust 4.12.0 → 4.14.0
- Bumped tray-icon 0.21.3 → 0.22.0

## [2.0.1] - 2026-03-27

### Changed
- Updated all dependencies to latest versions, notably eframe/egui 0.28 → 0.34 (sharper text rendering via the skrifa font engine) and ureq 2 → 3
- Removed direct native-tls dependency (now handled internally by ureq)

### Fixed
- Windows theming updated to egui 0.34 API (CornerRadius replaces Rounding)

### Infrastructure
- Added Dependabot for weekly Cargo dependency updates
- Added issue templates (bug report, feature request, question, translation)
- Added PR templates (general and translation-specific)
- Added funding configuration (GitHub Sponsors, Ko-fi)
- Added manual workflow dispatch trigger to CI

---

## [2.0.0] - 2026-03-27

Complete rewrite in Rust. See the [release notes](https://github.com/senjinthedragon/DragonFoxVPN/releases/tag/v2.0.0) for a full feature overview.

---

## [1.1.0] and earlier

Previous versions were written in Python. The Rust rewrite in 2.0.0 supersedes all prior versions entirely.

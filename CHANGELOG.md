# Changelog

## [Unreleased]

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

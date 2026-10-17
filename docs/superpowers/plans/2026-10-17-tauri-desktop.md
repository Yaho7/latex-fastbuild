# Texgo Desktop Implementation Plan

> Current policy: local static checks; GitHub Actions packages directly on every push. The earlier cloud test/QA pipeline described below is historical and has been removed from the automatic workflow.

**Goal:** Ship a Windows, macOS and Linux Tauri application that discovers images referenced by a LaTeX project, converts selected images to PDF and applies reversible reference replacements.

**Architecture:** `desktop/src-tauri` contains a framework-independent Rust processing core and a thin Tauri command adapter. `desktop/src` contains a React workbench. The existing CLI and its uncommitted changes remain outside the desktop commits.

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Vite, image, lopdf, svg2pdf, PDF.js, GitHub Actions.

## 1. Processing Core

- [x] Add regression tests in `desktop/src-tauri/tests/core.rs` for recursive input/include scanning, graphics paths, comments, Unicode paths, duplicate references, missing files, PDF conversion, cache invalidation, replacement conflicts and undo.
- [x] Implement `models.rs`, `scanner.rs`, `conversion.rs`, `transaction.rs` and `workspace.rs`; keep source images intact and limit source edits to resolved project files.
- [x] Add feature-gated Tauri commands and configuration; pure core tests run without GUI features in CI.
- [x] Review the scoped diff and commit the processing core. Cloud core tests passed on all three operating systems before the automation additions.

## 2. Workbench

- [x] Implement file selection, project scanning, image filtering and selection, quality controls, source/PDF previews, progress and cancellation.
- [x] Add replacement review and undo, with errors surfaced in Chinese.
- [x] Add focused frontend tests; cloud typecheck, tests and browser fixture QA passed.
- [x] Review the scoped diff and commit the workbench.
- [x] Apply the requested Apple-inspired visual redesign.
- [x] Verify redesigned workbench screenshots on GitHub Actions at 1440, 1024 and 390 widths.

## 3. Cloud Verification and Release

- [x] Add `desktop.yml`: frontend checks/tests, Rust tests on all three operating systems, Windows x64, Linux x64, macOS arm64 and x64 bundles.
- [x] Configure `desktop-v*` publication only after every packaging job succeeds, with installers and SHA256 checksums.
- [x] Document installation, quality semantics, scope limitations, release triggers and signing requirements.
- [ ] Push the desktop branch, inspect Actions results, fix failures through code commits, then tag and verify the first desktop Release.

## 4. Headless Developer Integration (Added by User)

- [x] Add independent `texgo-engine` executable with scan, conversion/build pipeline, watch and newline JSON-RPC 2.0.
- [x] Add versioned project JSON configuration, structured build results, concise diagnostics and complete captured logs.
- [x] Supply VS Code build/watch tasks and documentation; source replacement requires an explicit flag or confirmation field.
- [x] Persist generated-PDF-to-source mappings so background runs update edited original images after reference replacement.
- [x] Test headless operations on Actions; subsequently bundle the engine in every desktop installer instead of publishing separate engine archives.

## 5. User-Requested UI and Installer Revision

- [x] Replace bespoke controls with official shadcn/ui components, generated through the CLI on GitHub Actions.
- [x] Reorganize the workbench into a file table and a combined preview/settings inspector.
- [x] Bundle the independent engine as a Tauri external binary in the same desktop installer.
- [x] Verify new UI screenshots and all packaged CLI payloads on Actions; user accepted the shadcn workbench.

## 6. Simplified Packaging (Latest User Request)

- [x] Trigger direct four-platform packaging on every push, without a duplicate PR trigger.
- [x] Remove automatic frontend/Rust tests, browser QA, TeX fixtures, installer checks and dependency-lock transfer jobs.
- [x] Keep the bundled engine and publish only from desktop version tags.
- [x] Move static checks to local development; keep compilation and packaging on Actions.
- [ ] Publish with the simplified workflow. The old `desktop-v0.2.0` run was cancelled before publication; do not rewrite its tag.

## Acceptance

- Opening a real multi-file LaTeX fixture discovers only referenced images and reports unresolved paths.
- Raster PDF output retains alpha; SVG output retains vectors; quality settings affect cache identity.
- Applying replacements changes only path spans. Edited files cannot be overwritten by stale plans or undo.
- All installer jobs succeed before publication. Local static checks are allowed; compilation and packaging remain on Actions.
